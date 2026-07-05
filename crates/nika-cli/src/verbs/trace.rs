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
        let _ = writeln!(
            out,
            "  {:<w0$}  {}  {}  {:>w3$}  {}",
            c[0],
            theme.paint(Role::Dim, &format!("{:<w1$}", c[1])),
            dur_cell(&c[2], c[2].is_empty()),
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
    // The trace path is CLICKABLE on link-capable terminals (OSC-8).
    let _ = write!(
        line,
        " · full value: nika trace peek {} <task>",
        crate::verbs::linked_path(theme, trace)
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

/// One data edge: `from` fed `to` — `bytes` is the SOURCE task's
/// recorded output size when the trace carries it (the honest measure
/// of what flowed; a consumer may read a subpath of it).
struct FlowEdge {
    from: String,
    to: String,
    bytes: Option<usize>,
}

/// `nika trace flow <trace> <workflow>` — the data waterfall: edges
/// from the checked definition's bindings (`depends_on` + `${{ tasks.X
/// }}` references · the SAME over-collecting scan `--resume --from`
/// walks) × output sizes from the trace, plus the `outputs.<name>`
/// terminal edges. The time-waterfall shows WHEN; this shows WHY.
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
    out.push_str(&render_flow(&flow_edges(&wf, &view), theme));
    VerbOutput::ok(out)
}

/// Join plan bindings × trace sizes into the edge list (task edges in
/// definition order, then the `outputs.<name>` terminal edges).
fn flow_edges(wf: &nika_schema::raw::RawWorkflow, view: &RunView) -> Vec<FlowEdge> {
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    let size_of = |task: &str| -> Option<usize> {
        view.rows()
            .iter()
            .find(|r| r.id == task)
            .and_then(|r| r.output_json.as_deref())
            .map(str::len)
    };
    let mut edges = Vec::new();
    for task in &wf.tasks {
        let to = task.value.id.value.as_str();
        for from in nika_runtime::resume::referenced_upstreams(&task.value) {
            // The scan over-collects by design — keep only edges from a
            // REAL sibling task (never self).
            if from != to && ids.contains(&from.as_str()) {
                edges.push(FlowEdge {
                    bytes: size_of(&from),
                    from,
                    to: to.to_owned(),
                });
            }
        }
    }
    for (key, decl) in &wf.outputs {
        let template = match decl {
            nika_schema::types::OutputDecl::Untyped(v) => &v.value,
            nika_schema::types::OutputDecl::Typed { value, .. } => &value.value,
            // #[non_exhaustive] future forms carry no v0 template.
            _ => continue,
        };
        let mut refs = std::collections::BTreeSet::new();
        scan_task_refs(template, &mut refs);
        for from in refs {
            if ids.contains(&from.as_str()) {
                edges.push(FlowEdge {
                    bytes: size_of(&from),
                    from,
                    to: format!("outputs.{}", key.value),
                });
            }
        }
    }
    edges
}

/// Collect every `tasks.<snake_case_id>` token — the SAME boundary scan
/// the runtime's `referenced_upstreams` applies to task definitions
/// (task ids are checker-enforced `snake_case`; over-collection is
/// safe, the caller filters to real task ids).
fn scan_task_refs(text: &str, out: &mut std::collections::BTreeSet<String>) {
    let mut rest = text;
    while let Some(at) = rest.find("tasks.") {
        let after = &rest[at + "tasks.".len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(after.len());
        if end > 0 {
            out.insert(after[..end].to_owned());
        }
        rest = &after[end..];
    }
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
    let mut totals = format!("  {} edge(s)", edges.len());
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
    use crate::verbs::exit;

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
            "nika: v1\nworkflow: geo-audit\nmodel: mock/echo\ntasks:\n  - id: read_payload\n    invoke: { tool: \"nika:read\", args: { path: \"x.json\" } }\n  - id: audit\n    depends_on: [read_payload]\n    infer: { prompt: \"score ${{ tasks.read_payload.output }}\" }\noutputs:\n  geo_score: ${{ tasks.audit.output }}\n",
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
            text.contains("2 edge(s) · widest: read_payload→audit"),
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
}
