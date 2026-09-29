// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trace store reader — one scan over `.nika/traces/` yielding the
//! facts every retention consumer folds over (ADR-100): per-trace
//! workflow name · terminal state · size · age.
//!
//! One-voice: each file parses through the SAME tolerant reader
//! `--resume` and `trace show` fold through ([`recover_events`] — zero
//! parallel parsers); this module only FOLDS the recovered events into
//! retention facts. Fail-open is the law of [`scan`] (ADR-100): a file that
//! will not read or parse is SKIPPED — never counted, never collected,
//! never an error that blocks a run.
//!
//! [`survey`] is the same fold without fail-open, for readers that must
//! not mistake silence for absence (the lineage view): every `*.ndjson`
//! entry it cannot fold, every listing error and every doubt about a
//! folded journal (a torn suffix, a missing or conflicting identity) is
//! said; [`scan`] is its fail-open projection. A survey is syntactic: it
//! verifies no chain, no seal and no signature.
//!
//! Descended from `nika-cli`'s `verbs/trace/store` (2026-07-09 · the W0
//! trace descent); the CLI keeps its display vocabulary (the age cell)
//! and re-exports this seam at the old path.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nika_event::{Event, EventKind};

use crate::liveness::Liveness;

use crate::recover::{RecoveredTrace, recover_events};

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
    /// The trace this run CONTINUED (#1462 · `--resume`): the recorded
    /// journal's trace id, read from the opening frame's `resumed_from`
    /// field. `None` on a fresh run (or a journal older than the link).
    pub resumed_from: Option<String>,
    /// The journal's own run identity ([`crate::resume::trace_run_id`]):
    /// what a continuation names as its `resumed_from`. `None` when no
    /// opening frame names an execution.
    pub run_id: Option<String>,
    /// The project the journal was recorded for: the opening frame's
    /// `project_root_fingerprint`, when recorded.
    pub project: Option<String>,
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
            resumed_from: None,
            run_id: None,
            project: None,
        }
    }

    /// Attach the writer's liveness (a running trace only).
    #[must_use]
    pub const fn with_liveness(mut self, liveness: Liveness) -> Self {
        self.liveness = Some(liveness);
        self
    }

    /// Attach the continuation link (#1462 · a resumed leg only).
    #[must_use]
    pub fn with_resumed_from(mut self, resumed_from: Option<String>) -> Self {
        self.resumed_from = resumed_from;
        self
    }

    /// Attach the journal's own identity: its run and its project.
    #[must_use]
    pub fn with_identity(mut self, run_id: Option<String>, project: Option<String>) -> Self {
        self.run_id = run_id;
        self.project = project;
        self
    }

    /// The machine state word: the settlement's word for a run
    /// that settled, `running` for a live writer (or one this host cannot
    /// judge), `dead` for a writer that died — the evidence is incomplete,
    /// the run never settled (ADR-129: never a verdict on the run).
    /// Human listings may qualify unknown writers as `running?` (#1473).
    #[must_use]
    pub const fn state_word(&self) -> &'static str {
        match (self.state, self.liveness) {
            (TraceState::Running, Some(Liveness::Dead { .. })) => "dead",
            (state, _) => state.as_str(),
        }
    }
}

/// Why a `*.ndjson` entry could not be folded at all: an entry [`scan`]
/// skips, said by [`survey`] instead of dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipWhy {
    /// Its metadata or bytes could not be read (non-UTF-8 content reads
    /// `InvalidData`).
    Unreadable(std::io::ErrorKind),
    /// Not a regular file (a directory named `*.ndjson`).
    NotAFile,
    /// The file name is not UTF-8.
    NameNotUtf8,
    /// The ONE reader refused it — no readable opening event (its words).
    NoOpening(String),
}

/// One `*.ndjson` entry a [`survey`] could not fold.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Skipped {
    /// The entry.
    pub path: PathBuf,
    /// Why it was not folded.
    pub why: SkipWhy,
}

impl Skipped {
    /// Construct (INV-019).
    #[must_use]
    pub const fn new(path: PathBuf, why: SkipWhy) -> Self {
        Self { path, why }
    }
}

/// Why a folded journal's facts may be incomplete: the journal stays in
/// [`Survey::traces`], with the doubt beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DoubtWhy {
    /// The reader stopped at a torn or corrupt line: every later frame (a
    /// terminal among them) is lost — the reader's own note.
    TornSuffix(String),
    /// No `workflow_started` frame: identity and continuation link unknown.
    NoOpeningFrame,
    /// The opening frame names no execution: no run identity.
    NoRunIdentity,
    /// The frames carry more than one execution id.
    ConflictingIdentity,
}

/// One folded journal whose facts may be incomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Doubt {
    /// The journal (also in [`Survey::traces`]).
    pub path: PathBuf,
    /// Why its facts may be incomplete.
    pub why: DoubtWhy,
}

impl Doubt {
    /// Construct (INV-019).
    #[must_use]
    pub const fn new(path: PathBuf, why: DoubtWhy) -> Self {
        Self { path, why }
    }
}

/// A trace directory read without fail-open, in directory order (a survey
/// never ranks journals): [`scan`]'s fold with every failure kept.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Survey {
    /// Every journal the reader folded (recovered prefixes included).
    pub traces: Vec<TraceMeta>,
    /// The `*.ndjson` entries that could not be folded at all.
    pub skipped: Vec<Skipped>,
    /// The folded journals whose facts may be incomplete.
    pub doubts: Vec<Doubt>,
    /// The directory's own read errors: opening it, then each failed step
    /// of its listing.
    pub dir_errors: Vec<std::io::ErrorKind>,
}

impl Survey {
    /// An empty survey (INV-019) — what a listed, empty directory reads.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nothing this reader could see is unaccounted: the directory listed
    /// and every `*.ndjson` entry folded (a doubt is said, not missing).
    #[must_use]
    pub fn complete(&self) -> bool {
        self.dir_errors.is_empty() && self.skipped.is_empty()
    }
}

/// Scan a trace directory into retention facts, newest first.
///
/// The fail-open projection of [`survey`] (ADR-100): an unreadable file ·
/// a non-`.ndjson` name · a parse failure on the FIRST line — each skips
/// that entry; a torn journal keeps its recovered-prefix facts; a missing
/// directory scans empty. Never an error.
#[must_use]
pub fn scan(dir: &Path) -> Vec<TraceMeta> {
    let mut traces = survey(dir).traces;
    // Newest first · name tie-break — one deterministic order for every
    // consumer (ls renders it · the policy derives its own sorts).
    traces.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    traces
}

/// Survey a trace directory: every `*.ndjson` entry folded through the ONE
/// reader or said why not, every listing error kept. One pass, no retry.
#[must_use]
pub fn survey(dir: &Path) -> Survey {
    match std::fs::read_dir(dir) {
        Ok(entries) => survey_entries(entries.map(|entry| entry.map(|e| e.path()))),
        Err(error) => Survey {
            dir_errors: vec![error.kind()],
            ..Survey::default()
        },
    }
}

/// The per-entry fold behind [`survey`] (its listing-error seam).
fn survey_entries(entries: impl Iterator<Item = std::io::Result<PathBuf>>) -> Survey {
    let mut survey = Survey::default();
    for entry in entries {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                survey.dir_errors.push(error.kind());
                continue;
            }
        };
        if path.extension().is_none_or(|ext| ext != "ndjson") {
            continue;
        }
        match read_meta(&path) {
            Ok((meta, doubts)) => {
                let doubts = doubts.into_iter().map(|why| Doubt::new(path.clone(), why));
                survey.doubts.extend(doubts);
                survey.traces.push(meta);
            }
            Err(why) => survey.skipped.push(Skipped::new(path, why)),
        }
    }
    survey
}

/// Fold ONE `*.ndjson` entry into its retention facts and its doubts, or
/// say why it cannot be folded.
fn read_meta(path: &Path) -> Result<(TraceMeta, Vec<DoubtWhy>), SkipWhy> {
    let unreadable = |error: std::io::Error| SkipWhy::Unreadable(error.kind());
    let name = path.file_name().and_then(|n| n.to_str());
    let name = name.ok_or(SkipWhy::NameNotUtf8)?.to_owned();
    let meta = std::fs::metadata(path).map_err(unreadable)?;
    if !meta.is_file() {
        return Err(SkipWhy::NotAFile);
    }
    let modified = meta.modified().map_err(unreadable)?;
    let raw = std::fs::read_to_string(path).map_err(unreadable)?;
    // The ONE tolerant reader (`--resume` · `trace show` · here): a torn
    // tail keeps its valid prefix (and its note becomes a doubt); a file
    // with no readable first line is not a trace we can reason about.
    let recovered =
        recover_events(&raw, &name).map_err(|error| SkipWhy::NoOpening(error.to_string()))?;
    let (workflow, state, paused_task) = fold_facts(&recovered.events);
    // #1462 · the continuation link and the identity the opening frame
    // carries (a resumed leg names the trace it continued).
    let started = recovered
        .events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted);
    let field = |key: &str| started.and_then(|e| str_field(e, key)).map(str::to_owned);
    let run_id = crate::resume::trace_run_id(&recovered.events);
    let doubts = doubts(&recovered, started.is_some(), run_id.is_some());
    let mut facts = TraceMeta::new(
        path.to_path_buf(),
        name,
        workflow,
        state,
        paused_task,
        meta.len(),
        modified,
    )
    .with_resumed_from(field("resumed_from"))
    .with_identity(run_id, field("project_root_fingerprint"));
    // A running trace asks its lease (ADR-129): alive · dead · unknown.
    if state == TraceState::Running {
        facts = facts.with_liveness(crate::liveness::probe(path));
    }
    Ok((facts, doubts))
}

/// What a recovered journal cannot vouch for: a torn suffix, a missing
/// opening frame or run identity, more than one execution id.
fn doubts(recovered: &RecoveredTrace, opened: bool, identified: bool) -> Vec<DoubtWhy> {
    let mut doubts: Vec<DoubtWhy> = recovered
        .truncated_note
        .clone()
        .into_iter()
        .map(DoubtWhy::TornSuffix)
        .collect();
    if !opened {
        doubts.push(DoubtWhy::NoOpeningFrame);
    } else if !identified {
        doubts.push(DoubtWhy::NoRunIdentity);
    }
    let mut executions = recovered.events.iter().filter_map(|e| e.execution);
    if executions
        .next()
        .is_some_and(|first| executions.any(|other| other != first))
    {
        doubts.push(DoubtWhy::ConflictingIdentity);
    }
    doubts
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

/// The journal a run's `execution` (`exe-<uuid>`) and `trace` ids name under
/// `dir`, by the sink's own naming law: `<ts>-<last 4 hex>.ndjson`, or
/// `<ts>-<32 hex>.ndjson` on a same-second collision. Two runs in different
/// seconds can share a short id, so the first line's `execution.uuid` (the
/// stamp the sink writes on every line, read bounded) settles which file is
/// the run's. `None` when no file is. (Descended from Serve's trace verdict,
/// C6.)
#[must_use]
pub fn locate_trace(dir: &Path, execution: &str, trace: &str) -> Option<PathBuf> {
    let short = trace.get(trace.len().saturating_sub(4)..)?;
    let tails = [format!("-{short}.ndjson"), format!("-{trace}.ndjson")];
    let wanted = uuid_digits(execution);
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| tails.iter().any(|tail| name.ends_with(tail)))
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .find(|path| first_line_execution(path).is_some_and(|found| found == wanted))
}

/// A uuid's hex digits: a record stores `exe-<hyphenated>`, a journal
/// `{"uuid": "<hyphenated>"}`; the digits are the identity.
fn uuid_digits(id: &str) -> String {
    id.strip_prefix("exe-")
        .unwrap_or(id)
        .chars()
        .filter(char::is_ascii_hexdigit)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The first line's execution stamp, read bounded (one line, at most the
/// chain's line bound: the journal is untrusted input).
fn first_line_execution(path: &Path) -> Option<String> {
    use std::io::{BufRead as _, Read as _};
    let file = std::fs::File::open(path).ok()?;
    let bound = u64::try_from(crate::chain::MAX_LINE_BYTES).unwrap_or(u64::MAX);
    let mut reader = std::io::BufReader::new(file.take(bound.saturating_add(1)));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let value: serde_json::Value = serde_json::from_str(line.trim_end()).ok()?;
    value
        .get("execution")?
        .get("uuid")?
        .as_str()
        .map(uuid_digits)
}

#[cfg(test)]
mod locate_tests {
    use super::*;

    /// The identity digits: the record's `exe-` form and the journal's
    /// hyphenated uuid name the same run.
    #[test]
    fn uuid_digits_strip_the_prefix_and_the_hyphens() {
        assert_eq!(
            uuid_digits("exe-01a07812-3b0a-7ba0-b27e-a4893cac734f"),
            "01a078123b0a7ba0b27ea4893cac734f"
        );
        assert_eq!(
            uuid_digits("01A07812-3B0A-7BA0-B27E-A4893CAC734F"),
            "01a078123b0a7ba0b27ea4893cac734f"
        );
    }

    /// Two journals sharing a short id (different seconds) are told apart by
    /// the first line's execution stamp; a stranger's file is never the run's.
    #[test]
    fn locate_reads_the_first_lines_execution_to_settle_a_shared_short_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let line = |uuid: &str| {
            format!(
                "{{\"id\":{{\"uuid\":\"{uuid}\"}},\"timestamp\":1,\"kind\":\"workflow_started\",\"execution\":{{\"uuid\":\"{uuid}\"}},\"fields\":[],\"chain\":\"x\"}}\n"
            )
        };
        let mine = "01a07812-3b0a-7ba0-b27e-a4893cac734f";
        let other = "0000aaaa-0000-7000-8000-00000000734f";
        std::fs::write(
            dir.path().join("2026-01-01T00-00-00Z-734f.ndjson"),
            line(other),
        )
        .expect("other");
        std::fs::write(
            dir.path().join("2026-01-01T00-00-01Z-734f.ndjson"),
            line(mine),
        )
        .expect("mine");
        let found = locate_trace(dir.path(), &format!("exe-{mine}"), &uuid_digits(mine))
            .expect("the run's journal");
        assert!(
            found.ends_with("2026-01-01T00-00-01Z-734f.ndjson"),
            "{found:?}"
        );
        assert!(
            locate_trace(
                dir.path(),
                "exe-ffffffff-0000-7000-8000-000000000000",
                "ffffffff00007000800000000000734f"
            )
            .is_none(),
            "a run with no journal is not found in a stranger's file"
        );
    }
}

#[cfg(test)]
mod survey_tests;

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

    /// #1462 · a resumed leg's journal names the trace it continued on
    /// its opening frame; the store reads the link, and a fresh run has
    /// none (absent, never a guess).
    #[test]
    fn scan_reads_the_continuation_link() {
        let dir = temp_store("resumed-from");
        let mut leg2 = run_events("w", Some(EventKind::WorkflowCompleted));
        leg2[0] = leg2[0]
            .clone()
            .with_field(KeyValue::new("resumed_from", Value::String("c".repeat(32))));
        stage_trace(&dir, "leg2.ndjson", &ndjson(&leg2), Duration::from_secs(1));
        stage_trace(
            &dir,
            "leg1.ndjson",
            &ndjson(&run_events("w", Some(EventKind::WorkflowPaused))),
            Duration::from_secs(60),
        );
        let traces = scan(&dir);
        let of = |name: &str| traces.iter().find(|t| t.name == name).expect(name);
        assert_eq!(
            of("leg2.ndjson").resumed_from.as_deref(),
            Some("c".repeat(32).as_str()),
            "the continuation names the trace it resumed"
        );
        assert_eq!(
            of("leg1.ndjson").resumed_from,
            None,
            "a fresh run: no claim"
        );
        let _ = std::fs::remove_dir_all(dir);
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
