// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The explicit trace-store surface (ADR-100 D3) — `nika trace ls` ·
//! `nika trace rm`.
//!
//! `ls` renders the store's facts: age · size · workflow · terminal
//! state (`completed`/`failed`/`paused`) · the resume-candidate marker
//! (`★` — the newest trace of each workflow · exactly the set the GC
//! exemption protects). Where the retention policy DECIDES, this verb
//! SHOWS — one scan, one truth, two consumers.
//!
//! `rm` is the explicit removal lever the opportunistic GC never
//! reaches: one trace by name · `--older-than <dur>` · `--all`. The
//! pause contract holds here too — removing a `paused` trace REFUSES
//! without `--force` and names the unanswered task it would destroy.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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

/// What `nika trace rm` removes (D3): exactly one of the three forms —
/// clap enforces the exclusivity, this enum carries it.
#[derive(Debug, Clone)]
pub enum RmTarget {
    /// One trace — a bare name from `trace ls` (resolved in the store)
    /// or an explicit path.
    One(String),
    /// Every trace older than a duration (`7d` · `12h` · `30m` · `45s`).
    OlderThan(Duration),
    /// Every trace in the store.
    All,
}

/// Parse the `--older-than` duration form: `<N><unit>` with `s`/`m`/
/// `h`/`d` (`7d` · `12h` · `30m` · `45s`).
///
/// # Errors
///
/// A human-readable refusal naming the accepted form.
pub fn parse_older_than(raw: &str) -> Result<Duration, String> {
    let raw = raw.trim();
    let refuse = || format!("--older-than expects <N><unit> (s · m · h · d) — got `{raw}`");
    let (digits, unit) = raw.split_at(raw.len().saturating_sub(1));
    let n: u64 = digits.parse().map_err(|_| refuse())?;
    let seconds = match unit {
        "s" => n,
        "m" => n.saturating_mul(60),
        "h" => n.saturating_mul(3_600),
        "d" => n.saturating_mul(86_400),
        _ => return Err(refuse()),
    };
    Ok(Duration::from_secs(seconds))
}

/// `nika trace rm` — explicit removal from the workspace store.
#[must_use]
pub fn rm(target: &RmTarget, force: bool, theme: Theme) -> VerbOutput {
    rm_in(Path::new(store::TRACE_DIR), target, force, theme)
}

/// The dir-injected core (tests point it at a staged store).
pub(crate) fn rm_in(dir: &Path, target: &RmTarget, force: bool, theme: Theme) -> VerbOutput {
    match target {
        RmTarget::One(handle) => rm_one(dir, handle, force),
        RmTarget::OlderThan(cutoff) => rm_bulk(dir, Some(*cutoff), force, theme),
        RmTarget::All => rm_bulk(dir, None, force, theme),
    }
}

/// Remove ONE named trace. A `paused` trace refuses without `--force`
/// and names the unanswered task it carries (the ADR-100 D3 contract —
/// a forced removal must know what it destroys).
fn rm_one(dir: &Path, handle: &str, force: bool) -> VerbOutput {
    let Some(path) = resolve_handle(dir, handle) else {
        return VerbOutput::env(format!(
            "no trace `{handle}` — `nika trace ls` names the store"
        ));
    };
    // The paused refusal needs the trace's own facts. A file the reader
    // cannot fold carries no PROVEN obligation — an explicit rm of a
    // named unreadable file proceeds (it is junk to the whole surface).
    let meta = store::scan(dir)
        .into_iter()
        .find(|t| t.path == path)
        .or_else(|| scan_foreign(&path));
    if let Some(meta) = &meta
        && meta.state == TraceState::Paused
        && !force
    {
        return VerbOutput::env(paused_refusal(meta));
    }
    let bytes = meta.as_ref().map_or(0, |m| m.bytes);
    match std::fs::remove_file(&path) {
        Ok(()) => VerbOutput::ok(format!(
            "removed {} · {}",
            path.display(),
            retention::fmt_bytes(bytes)
        )),
        Err(e) => VerbOutput::env(format!("cannot remove {}: {e}", path.display())),
    }
}

/// Bulk removal (`--older-than` · `--all`): paused traces are SKIPPED
/// without `--force` — each skip is spoken (never a silent hold), the
/// rest report as one summary line.
fn rm_bulk(dir: &Path, cutoff: Option<Duration>, force: bool, theme: Theme) -> VerbOutput {
    use std::fmt::Write as _;
    let now = SystemTime::now();
    let matches: Vec<store::TraceMeta> = store::scan(dir)
        .into_iter()
        .filter(|t| match cutoff {
            Some(min_age) => now.duration_since(t.modified).unwrap_or_default() > min_age,
            None => true,
        })
        .collect();
    if matches.is_empty() {
        return VerbOutput::ok(format!(
            "  {}",
            theme.paint(Role::Dim, "nothing to remove — the store is already clean")
        ));
    }
    let (mut removed, mut freed) = (0usize, 0u64);
    let mut kept_paused: Vec<&store::TraceMeta> = Vec::new();
    for trace in &matches {
        if trace.state == TraceState::Paused && !force {
            kept_paused.push(trace);
            continue;
        }
        if std::fs::remove_file(&trace.path).is_ok() {
            removed += 1;
            freed += trace.bytes;
        }
    }
    let mut out = format!(
        "removed {removed} trace(s) · {} freed",
        retention::fmt_bytes(freed)
    );
    for trace in kept_paused {
        let _ = write!(out, "\n{}", paused_refusal(trace));
    }
    VerbOutput::ok(out)
}

/// The ADR-100 D3 refusal — names the obligation a removal would
/// destroy, then the lever.
fn paused_refusal(meta: &store::TraceMeta) -> String {
    let task = meta.paused_task.as_deref().unwrap_or("a prompt");
    format!(
        "{}: this trace carries an unanswered prompt for task `{task}` — --force removes it anyway",
        meta.name
    )
}

/// Resolve a `rm` handle: an explicit path wins; a bare name resolves
/// inside the store (the form `trace ls` prints).
fn resolve_handle(dir: &Path, handle: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(handle);
    if direct.is_file() {
        return Some(direct);
    }
    let in_store = dir.join(handle);
    in_store.is_file().then_some(in_store)
}

/// Facts for a trace OUTSIDE the store dir (an explicit path handle):
/// the same one-file fold `scan` applies per entry.
fn scan_foreign(path: &Path) -> Option<store::TraceMeta> {
    store::scan(path.parent()?)
        .into_iter()
        .find(|t| t.path == path)
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

    /// ADR-100 conformance fixture 4 · **forced-removal-speaks**:
    /// `trace rm` on a paused trace REFUSES without `--force` and names
    /// the unanswered task; `--force` removes it, knowingly.
    #[test]
    fn fixture_rm_paused_refuses_and_names_the_task() {
        let dir = temp_store("rm-paused");
        stage_trace(
            &dir,
            "gate.ndjson",
            &ndjson(&run_events("gatey", Some(EventKind::WorkflowPaused))),
            Duration::from_secs(60),
        );
        let refused = rm_in(&dir, &RmTarget::One("gate.ndjson".into()), false, plain());
        assert_eq!(refused.code, exit::ENV, "{}", refused.text);
        assert!(
            refused
                .text
                .contains("this trace carries an unanswered prompt for task `gate`"),
            "the refusal names what it protects: {}",
            refused.text
        );
        assert!(dir.join("gate.ndjson").exists(), "refusal removed nothing");

        let forced = rm_in(&dir, &RmTarget::One("gate.ndjson".into()), true, plain());
        assert_eq!(forced.code, exit::OK, "{}", forced.text);
        assert!(forced.text.contains("removed"), "{}", forced.text);
        assert!(!dir.join("gate.ndjson").exists(), "--force removes it");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `rm <trace>` resolves a bare `trace ls` name in the store, an
    /// explicit path anywhere, and refuses an unknown handle with the
    /// discovery pointer.
    #[test]
    fn rm_one_resolves_names_and_paths_and_refuses_ghosts() {
        let dir = temp_store("rm-one");
        let body = ndjson(&run_events("w", Some(EventKind::WorkflowCompleted)));
        stage_trace(&dir, "a.ndjson", &body, Duration::from_secs(60));
        let by_path = stage_trace(&dir, "b.ndjson", &body, Duration::from_secs(60));

        let named = rm_in(&dir, &RmTarget::One("a.ndjson".into()), false, plain());
        assert_eq!(named.code, exit::OK, "{}", named.text);
        assert!(!dir.join("a.ndjson").exists());

        let pathed = rm_in(
            Path::new("/somewhere/else"),
            &RmTarget::One(by_path.to_string_lossy().into_owned()),
            false,
            plain(),
        );
        assert_eq!(pathed.code, exit::OK, "explicit path wins: {}", pathed.text);
        assert!(!by_path.exists());

        let ghost = rm_in(&dir, &RmTarget::One("ghost.ndjson".into()), false, plain());
        assert_eq!(ghost.code, exit::ENV);
        assert!(
            ghost.text.contains("nika trace ls"),
            "teaches discovery: {}",
            ghost.text
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `--older-than` removes only what crossed the cutoff · skips a
    /// paused match OUT LOUD (never silently kept) · `--force` takes it.
    #[test]
    fn rm_older_than_respects_the_cutoff_and_speaks_paused_skips() {
        let dir = temp_store("rm-older");
        let done = ndjson(&run_events("w", Some(EventKind::WorkflowCompleted)));
        let gated = ndjson(&run_events("w", Some(EventKind::WorkflowPaused)));
        stage_trace(&dir, "old.ndjson", &done, Duration::from_secs(3 * 86_400));
        stage_trace(&dir, "gate.ndjson", &gated, Duration::from_secs(3 * 86_400));
        stage_trace(&dir, "fresh.ndjson", &done, Duration::from_secs(60));

        let out = rm_in(
            &dir,
            &RmTarget::OlderThan(Duration::from_secs(86_400)),
            false,
            plain(),
        );
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("removed 1 trace(s)"), "{}", out.text);
        assert!(
            out.text.contains("unanswered prompt for task `gate`"),
            "the kept obligation is spoken: {}",
            out.text
        );
        assert!(!dir.join("old.ndjson").exists());
        assert!(dir.join("gate.ndjson").exists(), "paused survives");
        assert!(dir.join("fresh.ndjson").exists(), "under the cutoff");

        let forced = rm_in(
            &dir,
            &RmTarget::OlderThan(Duration::from_secs(86_400)),
            true,
            plain(),
        );
        assert_eq!(forced.code, exit::OK);
        assert!(!dir.join("gate.ndjson").exists(), "--force takes it");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `--all` clears the store (same paused contract) and an already-
    /// clean store answers calmly.
    #[test]
    fn rm_all_clears_the_store_and_is_calm_when_empty() {
        let dir = temp_store("rm-all");
        let body = ndjson(&run_events("w", Some(EventKind::WorkflowCompleted)));
        stage_trace(&dir, "a.ndjson", &body, Duration::from_secs(60));
        stage_trace(&dir, "b.ndjson", &body, Duration::from_secs(120));
        let out = rm_in(&dir, &RmTarget::All, false, plain());
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("removed 2 trace(s)"), "{}", out.text);
        assert!(store::scan(&dir).is_empty());

        let calm = rm_in(&dir, &RmTarget::All, false, plain());
        assert_eq!(calm.code, exit::OK);
        assert!(calm.text.contains("nothing to remove"), "{}", calm.text);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The `--older-than` duration form: the four units parse · junk is
    /// refused with the accepted form named.
    #[test]
    fn older_than_duration_form_parses_and_refuses() {
        assert_eq!(parse_older_than("45s"), Ok(Duration::from_secs(45)));
        assert_eq!(parse_older_than("30m"), Ok(Duration::from_secs(1_800)));
        assert_eq!(parse_older_than("12h"), Ok(Duration::from_secs(43_200)));
        assert_eq!(parse_older_than("7d"), Ok(Duration::from_secs(604_800)));
        for junk in ["", "7", "d", "7w", "sept-jours", "-3d"] {
            let err = parse_older_than(junk).expect_err("refused");
            assert!(err.contains("--older-than expects"), "{err}");
        }
    }
}
