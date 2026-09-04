// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trace store reader — one scan over `.nika/traces/` yielding the
//! facts every retention consumer folds over (ADR-100): per-trace
//! workflow name · terminal state · size · age.
//!
//! One-voice: each file parses through the SAME tolerant reader
//! `--resume` and `trace show` fold through ([`recover_events`] — zero
//! parallel parsers); this module only FOLDS the recovered events into
//! retention facts. Fail-open is the law here (ADR-100): a file that
//! will not read or parse is SKIPPED — never counted, never collected,
//! never an error that blocks a run.
//!
//! Descended from `nika-cli`'s `verbs/trace/store` (2026-07-09 · the W0
//! trace descent); the CLI keeps its display vocabulary (the age cell)
//! and re-exports this seam at the old path.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nika_event::{Event, EventKind};

use crate::liveness::Liveness;

use crate::recover::recover_events;

/// Where run journals live, relative to the run's CWD (the workspace
/// root by convention — the editor extension watches exactly this
/// directory, and the trace writer appends here).
pub const TRACE_DIR: &str = ".nika/traces";

/// Where a FAILED run's semi-written outputs are quarantined (F-P14 ·
/// NEP-0014 · la dette du run), relative to the run's CWD — one
/// `<run-stamp>/` child per failed run, the stamp tying the debt to the
/// journal that attests it. Beside the journals on purpose: the
/// `run_sealed` line that carries the quarantine fold lives under
/// `traces/`, and the v2 cross-run finding reads both from here.
pub const QUARANTINE_DIR: &str = ".nika/quarantine";

/// A trace's terminal state — the LAST workflow-level terminal event
/// decides (ADR-100 · the ADR-099 journal vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraceState {
    /// `workflow_completed` — a finished, successful run (the settlement's
    /// `succeeded` · ADR-128: a kind is not a state word).
    Succeeded,
    /// `workflow_failed` — a finished, failed run.
    Failed,
    /// `workflow_cancelled` — a finished run stopped by decision.
    Cancelled,
    /// `workflow_paused` — an UNANSWERED human gate (an obligation,
    /// never garbage · the ADR-100 absolute exemption).
    Paused,
    /// No terminal event — a run in flight right now, or a torn trace
    /// from a crashed run (the age cap eventually clears the latter).
    Running,
}

impl TraceState {
    /// The state word `trace ls` prints (the report vocabulary).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Paused => "paused",
            Self::Running => "running",
        }
    }

    /// A finished run (either verdict) — the population the keep-last-N
    /// observability window rotates over (ADR-100 D1: "completed-run
    /// traces"). Paused runs are obligations; running ones aren't done.
    #[must_use]
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// One trace file's retention facts.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TraceMeta {
    /// Absolute-or-relative path (as scanned) — the removal handle.
    pub path: PathBuf,
    /// The bare file name (`2026-…-a3f2.ndjson`) — the display handle.
    pub name: String,
    /// The recorded workflow name (`workflow_started`'s field) — the
    /// per-workflow retention key. Empty when the trace never recorded
    /// one (torn at birth — age and budget still apply).
    pub workflow: String,
    /// The last workflow-level terminal event's verdict.
    pub state: TraceState,
    /// The awaiting task id when `state` is `Paused` — what a forced
    /// removal would destroy (the `trace rm` refusal names it).
    pub paused_task: Option<String>,
    /// File size in bytes (the budget's unit).
    pub bytes: u64,
    /// Last modification time (the age clock — a trace's mtime is its
    /// run's last write).
    pub modified: SystemTime,
    /// The writer's liveness, read from its lease (ADR-129): `Some` only
    /// while the trace is `Running` — alive, dead, or unknown when this
    /// host cannot say.
    pub liveness: Option<Liveness>,
}

impl TraceMeta {
    /// Assemble one trace's facts (invariant #19: every
    /// `#[non_exhaustive]` struct constructs through `new`, never a
    /// literal — the fact set stays free to grow).
    #[must_use]
    pub fn new(
        path: PathBuf,
        name: String,
        workflow: String,
        state: TraceState,
        paused_task: Option<String>,
        bytes: u64,
        modified: SystemTime,
    ) -> Self {
        Self {
            path,
            name,
            workflow,
            state,
            paused_task,
            bytes,
            modified,
            liveness: None,
        }
    }

    /// Attach the writer's liveness (a running trace only).
    #[must_use]
    pub const fn with_liveness(mut self, liveness: Liveness) -> Self {
        self.liveness = Some(liveness);
        self
    }

    /// The state word every door prints: the settlement's word for a run
    /// that settled, `running` for a live writer (or one this host cannot
    /// judge), `dead` for a writer that died — the evidence is incomplete,
    /// the run never settled (ADR-129: never a verdict on the run).
    #[must_use]
    pub const fn state_word(&self) -> &'static str {
        match (self.state, self.liveness) {
            (TraceState::Running, Some(Liveness::Dead { .. })) => "dead",
            (state, _) => state.as_str(),
        }
    }
}

/// Scan a trace directory into retention facts, newest first.
///
/// Fail-open per entry (ADR-100): an unreadable file · a non-`.ndjson`
/// name · a parse failure on the FIRST line — each skips that entry and
/// continues. A missing directory scans empty. Never an error.
#[must_use]
pub fn scan(dir: &Path) -> Vec<TraceMeta> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut traces: Vec<TraceMeta> = entries
        .flatten()
        .filter_map(|entry| read_meta(&entry.path()))
        .collect();
    // Newest first · name tie-break — one deterministic order for every
    // consumer (ls renders it · the policy derives its own sorts).
    traces.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    traces
}

/// Fold ONE file into its retention facts — `None` skips it (fail-open).
fn read_meta(path: &Path) -> Option<TraceMeta> {
    if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_owned();
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta.modified().ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    // The ONE tolerant reader (`--resume` · `trace show` · here): a torn
    // tail keeps its valid prefix; a file with no readable first line is
    // not a trace we can reason about — skipped, never collected.
    let recovered = recover_events(&raw, &name).ok()?;
    let (workflow, state, paused_task) = fold_facts(&recovered.events);
    let mut facts = TraceMeta::new(
        path.to_path_buf(),
        name,
        workflow,
        state,
        paused_task,
        meta.len(),
        modified,
    );
    // A running trace asks its lease (ADR-129): alive · dead · unknown.
    if state == TraceState::Running {
        facts = facts.with_liveness(crate::liveness::probe(path));
    }
    Some(facts)
}

/// Fold recovered events into (workflow name · terminal state · the
/// awaiting task): the FIRST `workflow_started` names the run; the
/// LAST workflow-level terminal event decides the state (none →
/// `Running`) and, when paused, names the unanswered task.
#[must_use]
pub fn fold_facts(events: &[Event]) -> (String, TraceState, Option<String>) {
    let workflow = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .and_then(|e| str_field(e, "workflow"))
        .unwrap_or_default()
        .to_owned();
    let mut state = TraceState::Running;
    let mut paused_task = None;
    for event in events {
        if event.kind.class() != nika_event::EventClass::Workflow || !event.kind.is_terminal() {
            continue;
        }
        state = match event.kind {
            EventKind::WorkflowCompleted => TraceState::Succeeded,
            EventKind::WorkflowFailed => TraceState::Failed,
            EventKind::WorkflowCancelled => TraceState::Cancelled,
            EventKind::WorkflowPaused => TraceState::Paused,
            // `#[non_exhaustive]` future terminal kinds: not one of the
            // states retention reasons about — treated as still running
            // (exempt from rotation · age and budget still bound it).
            _ => TraceState::Running,
        };
        paused_task = match state {
            TraceState::Paused => str_field(event, "task").map(str::to_owned),
            _ => None,
        };
    }
    (workflow, state, paused_task)
}

/// One string field off an event (the journal's additive KV vocabulary).
fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
        if let nika_types::resource::Value::String(s) = &kv.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

// ── The handle resolution + retention knobs (descended from
// nika-cli's verbs::trace::manage 2026-07-21 · the 15k wall) ─────────

/// `--older-than <N><unit>` — the duration parse (`45s` · `30m` ·
/// `12h` · `7d`). Split on CHARS, not bytes — a multi-byte trailing
/// unit (`7é`) must refuse, never panic on a char boundary.
///
/// # Errors
///
/// A human-readable refusal naming the accepted form.
pub fn parse_older_than(raw: &str) -> Result<std::time::Duration, String> {
    let raw = raw.trim();
    let refuse = || format!("--older-than expects <N><unit> (s · m · h · d) — got `{raw}`");
    let mut digits = raw.chars();
    let unit = digits.next_back().ok_or_else(refuse)?;
    let n: u64 = digits.as_str().parse().map_err(|_| refuse())?;
    let seconds = match unit {
        's' => n,
        'm' => n.saturating_mul(60),
        'h' => n.saturating_mul(3_600),
        'd' => n.saturating_mul(86_400),
        _ => return Err(refuse()),
    };
    Ok(std::time::Duration::from_secs(seconds))
}

/// A handle resolves to a file: the path itself first, then inside
/// `dir` (the store). `None` when neither exists.
#[must_use]
pub fn resolve_handle(dir: &std::path::Path, handle: &str) -> Option<std::path::PathBuf> {
    let direct = std::path::PathBuf::from(handle);
    if direct.is_file() {
        return Some(direct);
    }
    let in_store = dir.join(handle);
    in_store.is_file().then_some(in_store)
}

/// The newest trace in `dir` (mtime · name tie-break from [`scan`]) —
/// `None` on an empty store.
#[must_use]
pub fn latest_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    scan(dir).into_iter().next().map(|meta| meta.path)
}

/// Facts for a trace OUTSIDE the store dir (an explicit path handle):
/// the same one-file fold [`scan`] applies per entry.
#[must_use]
pub fn scan_foreign(path: &std::path::Path) -> Option<TraceMeta> {
    scan(path.parent()?).into_iter().find(|t| t.path == path)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::Duration;

    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value};
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    /// One journal event (nil id · fixed clock) with a task/workflow field.
    pub(crate) fn event(kind: EventKind, key: &str, name: &str, ms: u64) -> Event {
        Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(ms), kind)
            .with_field(KeyValue::new(key, Value::String(name.to_owned())))
    }

    /// Serialize events as one NDJSON trace body.
    pub(crate) fn ndjson(events: &[Event]) -> String {
        let mut body = String::new();
        for ev in events {
            body.push_str(&serde_json::to_string(ev).expect("event serializes"));
            body.push('\n');
        }
        body
    }

    /// The journal of one run: started(workflow) · a task · a terminal.
    pub(crate) fn run_events(workflow: &str, terminal: Option<EventKind>) -> Vec<Event> {
        let mut events = vec![
            event(EventKind::WorkflowStarted, "workflow", workflow, 0),
            event(EventKind::TaskCompleted, "task", "step", 10),
        ];
        if let Some(kind) = terminal {
            events.push(event(kind, "task", "gate", 20));
        }
        events
    }

    /// A fresh per-test trace directory under the cargo tmp root.
    pub(crate) fn temp_store(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join("nika-dap-trace-store");
        let dir = base.join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("store dir");
        dir
    }

    /// Stage one trace file and BACKDATE its mtime by `age` — the age
    /// clock the policy reads is the file's mtime.
    pub(crate) fn stage_trace(dir: &Path, name: &str, body: &str, age: Duration) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("trace staged");
        let mtime = SystemTime::now()
            .checked_sub(age)
            .expect("test age fits the clock");
        let file = std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("reopen for times");
        file.set_times(std::fs::FileTimes::new().set_modified(mtime))
            .expect("mtime set");
        path
    }

    /// The scan folds workflow name · terminal state · size per file,
    /// newest first.
    #[test]
    fn scan_folds_workflow_state_and_size() {
        let dir = temp_store("fold");
        stage_trace(
            &dir,
            "old-ok.ndjson",
            &ndjson(&run_events("veille", Some(EventKind::WorkflowCompleted))),
            Duration::from_secs(3_600),
        );
        stage_trace(
            &dir,
            "new-fail.ndjson",
            &ndjson(&run_events("veille", Some(EventKind::WorkflowFailed))),
            Duration::from_secs(60),
        );
        let traces = scan(&dir);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].name, "new-fail.ndjson", "newest first");
        assert_eq!(traces[0].state, TraceState::Failed);
        assert_eq!(traces[1].state, TraceState::Succeeded);
        assert!(traces.iter().all(|t| t.workflow == "veille"));
        assert!(traces.iter().all(|t| t.bytes > 0), "real sizes");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The LAST terminal event decides — a resumed-then-completed
    /// journal (paused, then completed appended) reads completed.
    #[test]
    fn paused_state_holds_and_the_last_terminal_wins() {
        let dir = temp_store("paused");
        stage_trace(
            &dir,
            "paused.ndjson",
            &ndjson(&run_events("gatey", Some(EventKind::WorkflowPaused))),
            Duration::from_secs(10),
        );
        let mut answered = run_events("gatey", Some(EventKind::WorkflowPaused));
        answered.push(event(EventKind::WorkflowCompleted, "workflow", "gatey", 30));
        stage_trace(
            &dir,
            "answered.ndjson",
            &ndjson(&answered),
            Duration::from_secs(5),
        );
        let traces = scan(&dir);
        let paused = traces
            .iter()
            .find(|t| t.name == "paused.ndjson")
            .expect("scanned");
        assert_eq!(paused.state, TraceState::Paused);
        let answered = traces
            .iter()
            .find(|t| t.name == "answered.ndjson")
            .expect("scanned");
        assert_eq!(answered.state, TraceState::Succeeded, "last wins");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Fail-open (ADR-100): garbage files · foreign extensions · a
    /// missing dir — each scans to nothing, never an error.
    #[test]
    fn scan_is_fail_open_on_garbage_and_missing_dir() {
        let dir = temp_store("failopen");
        stage_trace(&dir, "junk.ndjson", "{not json\n", Duration::from_secs(5));
        stage_trace(&dir, "notes.txt", "hello", Duration::from_secs(5));
        stage_trace(
            &dir,
            "ok.ndjson",
            &ndjson(&run_events("w", Some(EventKind::WorkflowCompleted))),
            Duration::from_secs(5),
        );
        let traces = scan(&dir);
        assert_eq!(traces.len(), 1, "only the readable trace counts");
        assert_eq!(traces[0].name, "ok.ndjson");
        assert!(scan(Path::new("/nonexistent/traces")).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A trace with no terminal event reads `running` (in flight · or
    /// torn — rotation exempts it, age still bounds it).
    /// ADR-129 · a running trace whose lease nobody holds reads `dead` on
    /// this host; one with no lease reads `unknown` — never a guess.
    #[cfg(unix)]
    #[test]
    fn a_running_trace_asks_its_lease_before_speaking() {
        let dir = temp_store("lease");
        let path = stage_trace(
            &dir,
            "dead.ndjson",
            &ndjson(&run_events("w", None)),
            Duration::from_secs(60),
        );
        let traces = scan(&dir);
        assert_eq!(
            traces[0].liveness,
            Some(Liveness::Unknown),
            "no lease → unknown"
        );
        assert_eq!(traces[0].state_word(), "running");
        std::fs::write(
            crate::liveness::lease_path(&path),
            format!(
                "{{\"pid\":1,\"host\":\"{}\"}}\n",
                crate::liveness::host_name()
            ),
        )
        .expect("a lease nobody holds");
        let traces = scan(&dir);
        assert_eq!(traces[0].liveness, Some(Liveness::Dead { pid: 1 }));
        assert_eq!(traces[0].state_word(), "dead");
        assert_eq!(
            traces[0].state,
            TraceState::Running,
            "never a verdict on the run"
        );
    }

    #[test]
    fn no_terminal_event_reads_running() {
        let dir = temp_store("running");
        stage_trace(
            &dir,
            "live.ndjson",
            &ndjson(&run_events("w", None)),
            Duration::from_secs(1),
        );
        let traces = scan(&dir);
        assert_eq!(traces[0].state, TraceState::Running);
        assert!(!traces[0].state.is_finished());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The rotation population is FINISHED runs only — a paused or
    /// running trace is never part of the observability window.
    #[test]
    fn finished_covers_both_verdicts_and_cancellation_only() {
        assert!(TraceState::Succeeded.is_finished());
        assert!(TraceState::Failed.is_finished());
        assert!(TraceState::Cancelled.is_finished());
        assert!(!TraceState::Paused.is_finished());
        assert!(!TraceState::Running.is_finished());
    }
}
