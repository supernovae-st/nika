// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The explicit trace-store surface (ADR-100 D3) — `nika trace ls`.
//!
//! `ls` renders the store's facts: age · size · workflow · terminal
//! state (`completed`/`failed`/`paused`) · the resume-candidate marker
//! (`★` — the newest trace of each workflow · exactly the set the GC
//! exemption protects). Where the retention policy DECIDES, this verb
//! SHOWS — one scan, one truth, two consumers.

use std::path::Path;
use std::time::SystemTime;

use crate::display::shape;
use crate::display::theme::{Role, Theme};
use crate::verbs::VerbOutput;

use super::retention;
use super::store::{self, TraceMeta, TraceState};

/// `nika trace ls` — list the workspace trace store (`.nika/traces/`).
#[must_use]
pub fn ls(theme: Theme) -> VerbOutput {
    ls_in(Path::new(store::TRACE_DIR), theme)
}

/// The dir-injected core (tests point it at a staged store).
pub(crate) fn ls_in(dir: &Path, theme: Theme) -> VerbOutput {
    let traces = store::scan(dir);
    VerbOutput::ok(render_ls(&traces, dir, SystemTime::now(), theme))
}

/// Render the store table + the totals line. Pure over the scanned
/// facts + an injected clock (the age column).
fn render_ls(traces: &[TraceMeta], dir: &Path, now: SystemTime, theme: Theme) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if traces.is_empty() {
        let line = format!("no traces in {}", dir.display());
        let _ = writeln!(out, "  {}", theme.paint(Role::Dim, &line));
        return out;
    }
    let newest = retention::newest_per_workflow(traces);
    let star = if theme.ascii { "*" } else { "★" };
    let cells: Vec<[String; 4]> = traces
        .iter()
        .map(|t| {
            [
                t.name.clone(),
                store::fmt_age(now.duration_since(t.modified).unwrap_or_default()),
                shape::fmt_bytes(usize::try_from(t.bytes).unwrap_or(usize::MAX)),
                dash_if_empty(&t.workflow, theme),
            ]
        })
        .collect();
    let header = ["trace", "age", "size", "workflow"];
    let width = |i: usize| {
        cells
            .iter()
            .map(|c| c[i].chars().count())
            .chain(std::iter::once(header[i].len()))
            .max()
            .unwrap_or(0)
    };
    let (w0, w1, w2, w3) = (width(0), width(1), width(2), width(3));
    let head = format!(
        "  {:<w0$}  {:>w1$}  {:>w2$}  {:<w3$}  state",
        header[0], header[1], header[2], header[3],
    );
    let _ = writeln!(out, "{}", theme.paint(Role::Dim, &head));
    for (i, (trace, c)) in traces.iter().zip(&cells).enumerate() {
        let marker = if newest.contains(&i) {
            format!(" {}", theme.paint(Role::Accent, star))
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  {:<w0$}  {:>w1$}  {:>w2$}  {:<w3$}  {}{marker}",
            c[0],
            theme.paint(Role::Dim, &c[1]),
            c[2],
            c[3],
            state_cell(trace.state, theme),
        );
    }
    let _ = writeln!(out, "{}", totals_line(traces, dir, theme));
    out
}

/// The state cell, painted semantically (never decoratively): a paused
/// trace is an OBLIGATION (warn) · a failure is red · the rest stay
/// calm. Sober registers (no colour) keep the bare word.
fn state_cell(state: TraceState, theme: Theme) -> String {
    let role = match state {
        TraceState::Completed => Role::Good,
        TraceState::Failed => Role::Bad,
        TraceState::Paused => Role::Warn,
        TraceState::Cancelled => Role::Dim,
        TraceState::Running => Role::Accent,
    };
    theme.paint(role, state.as_str())
}

/// The honest empty cell for a trace that never recorded its workflow
/// name (torn at birth) — `-` under `--ascii`.
fn dash_if_empty(workflow: &str, theme: Theme) -> String {
    if workflow.is_empty() {
        if theme.ascii { "-" } else { "—" }.to_owned()
    } else {
        workflow.to_owned()
    }
}

/// The closing line: `N trace(s) · <size> · <paused obligations> · <dir>`
/// — the paused count surfaces only when obligations exist.
fn totals_line(traces: &[TraceMeta], dir: &Path, theme: Theme) -> String {
    use std::fmt::Write as _;
    let bytes: u64 = traces.iter().map(|t| t.bytes).sum();
    let mut line = format!(
        "  {} trace(s) · {}",
        traces.len(),
        retention::fmt_bytes(bytes)
    );
    let paused = traces
        .iter()
        .filter(|t| t.state == TraceState::Paused)
        .count();
    if paused > 0 {
        let _ = write!(line, " · {paused} paused");
    }
    let _ = write!(line, " · {}", dir.display());
    theme.paint(Role::Dim, &line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;
    use crate::verbs::trace::store::tests::{ndjson, run_events, stage_trace, temp_store};
    use nika_event::EventKind;
    use std::time::Duration;

    fn plain() -> Theme {
        Theme::new(false, false, false)
    }

    /// The ls table: one row per trace (newest first) · age · size ·
    /// workflow · state — the paused trace says `paused` (ADR-100
    /// fixture 1's surface half) and the totals line counts the
    /// obligation.
    #[test]
    fn ls_marks_paused_and_counts_the_obligation() {
        let dir = temp_store("ls-paused");
        stage_trace(
            &dir,
            "gate.ndjson",
            &ndjson(&run_events("gatey", Some(EventKind::WorkflowPaused))),
            Duration::from_secs(2 * 3_600),
        );
        stage_trace(
            &dir,
            "ok.ndjson",
            &ndjson(&run_events("veille", Some(EventKind::WorkflowCompleted))),
            Duration::from_secs(60),
        );
        let out = ls_in(&dir, plain());
        assert_eq!(out.code, exit::OK);
        let text = &out.text;
        assert!(text.contains("trace") && text.contains("state"), "{text}");
        assert!(text.contains("gate.ndjson"), "{text}");
        assert!(text.contains("paused"), "the obligation is visible: {text}");
        assert!(text.contains("completed"), "{text}");
        assert!(text.contains("2h") && text.contains("1m"), "ages: {text}");
        assert!(text.contains("2 trace(s)"), "{text}");
        assert!(text.contains("1 paused"), "totals count it: {text}");
        let ok_line = text
            .lines()
            .find(|l| l.contains("ok.ndjson"))
            .expect("row exists");
        let gate_line = text
            .lines()
            .find(|l| l.contains("gate.ndjson"))
            .expect("row exists");
        assert!(
            text.find("ok.ndjson") < text.find("gate.ndjson"),
            "newest first: {text}"
        );
        assert!(ok_line.contains("veille") && gate_line.contains("gatey"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The resume-candidate marker rides the NEWEST trace of each
    /// workflow — exactly the GC-exempt set, so what ls stars is what
    /// collection spares.
    #[test]
    fn ls_stars_the_newest_of_each_workflow() {
        let dir = temp_store("ls-star");
        let body = ndjson(&run_events("veille", Some(EventKind::WorkflowCompleted)));
        stage_trace(&dir, "old.ndjson", &body, Duration::from_secs(7_200));
        stage_trace(&dir, "new.ndjson", &body, Duration::from_secs(60));
        let out = ls_in(&dir, plain());
        let starred: Vec<&str> = out.text.lines().filter(|l| l.contains('★')).collect();
        assert_eq!(starred.len(), 1, "one workflow → one star: {}", out.text);
        assert!(starred[0].contains("new.ndjson"), "{}", out.text);
        // ASCII parity: the marker degrades to `*` · zero unicode leaks.
        let ascii = ls_in(&dir, Theme::new(false, true, false));
        assert!(ascii.text.contains('*'), "{}", ascii.text);
        for glyph in ['★', '—'] {
            assert!(!ascii.text.contains(glyph), "unicode leaked: {glyph}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An empty (or absent) store is a calm empty state — exit 0, the
    /// dir named, nothing invented.
    #[test]
    fn ls_empty_store_is_calm() {
        let dir = temp_store("ls-empty");
        let out = ls_in(&dir, plain());
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("no traces in"), "{}", out.text);
        let absent = ls_in(Path::new("/nonexistent/traces"), plain());
        assert_eq!(absent.code, exit::OK, "a missing dir is an empty store");
        let _ = std::fs::remove_dir_all(dir);
    }
}
