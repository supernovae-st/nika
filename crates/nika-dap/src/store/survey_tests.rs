// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The non-suppressing survey (C7c): every `*.ndjson` entry is folded or
//! said, every listing error is kept, every doubt about a folded journal
//! sits beside it — and [`scan`] stays its fail-open projection, fact for
//! fact.

use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use nika_types::id::ExecutionId;
use nika_types::resource::{KeyValue, Value};
use uuid::Uuid;

use super::tests::{event, ndjson, run_events, stage_trace, temp_store};
use super::*;

/// The historical facts `scan` returned before the survey existed.
type Facts = (
    PathBuf,
    String,
    String,
    TraceState,
    Option<String>,
    u64,
    SystemTime,
    Option<Liveness>,
    Option<String>,
);

fn facts(trace: &TraceMeta) -> Facts {
    (
        trace.path.clone(),
        trace.name.clone(),
        trace.workflow.clone(),
        trace.state,
        trace.paused_task.clone(),
        trace.bytes,
        trace.modified,
        trace.liveness,
        trace.resumed_from.clone(),
    )
}

/// `scan` is `survey().traces` in the historical order, with the same
/// facts for every journal.
fn assert_scan_parity(dir: &Path) {
    let mut surveyed = survey(dir).traces;
    surveyed.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    let scanned: Vec<Facts> = scan(dir).iter().map(facts).collect();
    let projected: Vec<Facts> = surveyed.iter().map(facts).collect();
    assert_eq!(
        scanned, projected,
        "scan is the survey's fail-open projection"
    );
}

/// One run's journal carrying an execution on every frame, like the sink.
fn identified(workflow: &str, execution: u128, terminal: Option<EventKind>) -> Vec<Event> {
    let id = ExecutionId::new(Uuid::from_u128(execution));
    let mut events = run_events(workflow, terminal);
    events[0] = events[0].clone().with_field(KeyValue::new(
        "project_root_fingerprint",
        Value::String("f".repeat(64)),
    ));
    events.into_iter().map(|e| e.with_execution(id)).collect()
}

fn doubts_of(survey: &Survey, name: &str) -> Vec<DoubtWhy> {
    survey
        .doubts
        .iter()
        .filter(|d| d.path.file_name().and_then(|n| n.to_str()) == Some(name))
        .map(|d| d.why.clone())
        .collect()
}

fn skip_of(survey: &Survey, name: &str) -> Option<SkipWhy> {
    survey
        .skipped
        .iter()
        .find(|s| s.path.file_name().and_then(|n| n.to_str()) == Some(name))
        .map(|s| s.why.clone())
}

/// Every entry `scan` skips is said, with the reader's own words for the
/// journals it refuses; a foreign extension is not an entry at all.
#[test]
fn every_skip_is_said_and_scan_still_skips_it() {
    let dir = temp_store("survey-skips");
    stage_trace(&dir, "junk.ndjson", "{not json\n", Duration::from_secs(5));
    stage_trace(&dir, "empty.ndjson", "", Duration::from_secs(5));
    stage_trace(&dir, "notes.txt", "hello", Duration::from_secs(5));
    std::fs::create_dir(dir.join("folder.ndjson")).expect("a directory named like a journal");
    let ok = ndjson(&identified("w", 1, Some(EventKind::WorkflowCompleted)));
    stage_trace(&dir, "ok.ndjson", &ok, Duration::from_secs(5));
    let survey = survey(&dir);
    assert!(!survey.complete(), "three entries were not folded");
    assert!(survey.dir_errors.is_empty());
    assert_eq!(survey.traces.len(), 1);
    assert_eq!(survey.skipped.len(), 3, "{:?}", survey.skipped);
    assert!(matches!(
        skip_of(&survey, "junk.ndjson"),
        Some(SkipWhy::NoOpening(ref why)) if why.starts_with("junk.ndjson:1: bad event")
    ));
    assert!(matches!(
        skip_of(&survey, "empty.ndjson"),
        Some(SkipWhy::NoOpening(ref why)) if why == "empty.ndjson: empty trace"
    ));
    assert_eq!(skip_of(&survey, "folder.ndjson"), Some(SkipWhy::NotAFile));
    assert_eq!(skip_of(&survey, "notes.txt"), None, "not a journal entry");
    assert!(survey.doubts.is_empty(), "an identified complete journal");
    assert_eq!(
        scan(&dir).len(),
        1,
        "scan skips exactly what the survey says"
    );
    assert_scan_parity(&dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// Invalid UTF-8 content is unreadable, said with the I/O kind.
#[test]
fn unreadable_content_is_said_with_its_kind() {
    let dir = temp_store("survey-bytes");
    std::fs::write(dir.join("bytes.ndjson"), [0xff_u8, 0xfe, b'\n']).expect("staged");
    let survey = survey(&dir);
    assert_eq!(
        skip_of(&survey, "bytes.ndjson"),
        Some(SkipWhy::Unreadable(ErrorKind::InvalidData))
    );
    assert!(scan(&dir).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

/// A torn journal stays in `scan` with its recovered-prefix facts, and the
/// survey says the tear beside it: its terminal may be lost.
#[test]
fn a_torn_journal_keeps_its_prefix_and_gains_a_doubt() {
    let dir = temp_store("survey-torn");
    let mut body = ndjson(&identified("w", 2, None));
    body.push_str("{\"id\":{\"uuid\":\"torn");
    stage_trace(&dir, "torn.ndjson", &body, Duration::from_secs(5));
    let survey = survey(&dir);
    assert!(survey.complete(), "folded: the tear is a doubt, not a skip");
    let torn = &survey.traces[0];
    assert_eq!(
        torn.state,
        TraceState::Running,
        "the prefix has no terminal"
    );
    assert!(matches!(
        doubts_of(&survey, "torn.ndjson").as_slice(),
        [DoubtWhy::TornSuffix(note)] if note.contains("trace truncated")
    ));
    let scanned = scan(&dir);
    assert_eq!(
        scanned.len(),
        1,
        "scan keeps the recovered prefix, as before"
    );
    assert_eq!(scanned[0].state, TraceState::Running);
    assert_scan_parity(&dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// The reader's own settlement rule: a valid `run_settled` envelope is not
/// corruption; a torn envelope is.
#[test]
fn a_settlement_envelope_is_not_a_tear_but_a_torn_one_is() {
    let dir = temp_store("survey-settled");
    let mut settled = ndjson(&identified("w", 3, Some(EventKind::WorkflowPaused)));
    settled.push_str(
        "{\"kind\":\"run_settled\",\"status\":\"paused\",\"outputs\":{},\"chain\":\"c\"}\n",
    );
    stage_trace(&dir, "settled.ndjson", &settled, Duration::from_secs(5));
    let mut torn = ndjson(&identified("w", 4, Some(EventKind::WorkflowPaused)));
    torn.push_str("{\"kind\":\"run_settled\",\"status\":\"pau");
    stage_trace(&dir, "torn-envelope.ndjson", &torn, Duration::from_secs(5));
    let survey = survey(&dir);
    assert!(doubts_of(&survey, "settled.ndjson").is_empty());
    assert!(matches!(
        doubts_of(&survey, "torn-envelope.ndjson").as_slice(),
        [DoubtWhy::TornSuffix(_)]
    ));
    assert!(survey.traces.iter().all(|t| t.state == TraceState::Paused));
    assert_scan_parity(&dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// The identity a continuation names, read from the opening frame, and the
/// doubts when it is missing or ambiguous.
#[test]
fn identity_facts_and_identity_doubts() {
    let dir = temp_store("survey-identity");
    let named = identified("w", 0x01a0_e818_d0b1_72ba_af46_9009_01e0_157a, None);
    stage_trace(
        &dir,
        "named.ndjson",
        &ndjson(&named),
        Duration::from_secs(5),
    );
    let unnamed = run_events("w", Some(EventKind::WorkflowCompleted));
    stage_trace(
        &dir,
        "unnamed.ndjson",
        &ndjson(&unnamed),
        Duration::from_secs(5),
    );
    let opening = vec![
        event(EventKind::TaskCompleted, "task", "step", 10),
        event(EventKind::WorkflowCompleted, "task", "gate", 20),
    ];
    stage_trace(
        &dir,
        "no-opening.ndjson",
        &ndjson(&opening),
        Duration::from_secs(5),
    );
    let mut twice = identified("w", 5, None);
    twice.extend(identified("w", 6, Some(EventKind::WorkflowCompleted)));
    stage_trace(
        &dir,
        "twice.ndjson",
        &ndjson(&twice),
        Duration::from_secs(5),
    );
    let survey = survey(&dir);
    let named = survey
        .traces
        .iter()
        .find(|t| t.name == "named.ndjson")
        .expect("named");
    assert_eq!(
        named.run_id.as_deref(),
        Some("01a0e818d0b172baaf46900901e0157a")
    );
    assert_eq!(named.project.as_deref(), Some("f".repeat(64).as_str()));
    assert!(doubts_of(&survey, "named.ndjson").is_empty());
    assert_eq!(
        doubts_of(&survey, "unnamed.ndjson"),
        vec![DoubtWhy::NoRunIdentity]
    );
    assert_eq!(
        doubts_of(&survey, "no-opening.ndjson"),
        vec![DoubtWhy::NoOpeningFrame]
    );
    assert_eq!(
        doubts_of(&survey, "twice.ndjson"),
        vec![DoubtWhy::ConflictingIdentity]
    );
    let twice = survey
        .traces
        .iter()
        .find(|t| t.name == "twice.ndjson")
        .expect("twice");
    assert_eq!(twice.state, TraceState::Succeeded, "facts as before");
    assert_eq!(
        scan(&dir).len(),
        4,
        "doubts never remove a journal from scan"
    );
    assert_scan_parity(&dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// Listing errors are kept: a missing directory, and each failed step of a
/// listing (through the private seam: a real one cannot be provoked).
#[test]
fn listing_errors_are_kept_in_order() {
    let missing = survey(Path::new("/nonexistent/nika/traces"));
    assert_eq!(missing.dir_errors, vec![ErrorKind::NotFound]);
    assert!(!missing.complete());
    assert!(scan(Path::new("/nonexistent/nika/traces")).is_empty());
    let dir = temp_store("survey-listing");
    let ok = stage_trace(
        &dir,
        "ok.ndjson",
        &ndjson(&identified("w", 7, Some(EventKind::WorkflowCompleted))),
        Duration::from_secs(5),
    );
    let entries = vec![
        Err(std::io::Error::from(ErrorKind::Interrupted)),
        Ok(ok),
        Err(std::io::Error::from(ErrorKind::Other)),
    ];
    let survey = survey_entries(entries.into_iter());
    assert_eq!(
        survey.dir_errors,
        vec![ErrorKind::Interrupted, ErrorKind::Other]
    );
    assert_eq!(
        survey.traces.len(),
        1,
        "a listing error never hides the entries it did list"
    );
    assert!(!survey.complete());
    let _ = std::fs::remove_dir_all(dir);
}

/// A directory the reader may not list is said with its kind.
#[cfg(unix)]
#[test]
fn an_unlistable_directory_is_said() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_store("survey-denied");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");
    let listed = std::fs::read_dir(&dir).is_ok();
    let survey = survey(&dir);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod back");
    if !listed {
        assert_eq!(survey.dir_errors, vec![ErrorKind::PermissionDenied]);
        assert!(!survey.complete());
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// The name is judged before any I/O, so the seam proves it on every
/// filesystem (APFS refuses to store such a name at all).
#[cfg(unix)]
#[test]
fn a_non_utf8_name_is_said_before_any_read() {
    use std::os::unix::ffi::OsStrExt as _;
    let name = std::ffi::OsStr::from_bytes(b"/nonexistent/\xff-journal.ndjson");
    let survey = survey_entries(std::iter::once(Ok(PathBuf::from(name))));
    assert_eq!(survey.skipped.len(), 1);
    assert_eq!(survey.skipped[0].why, SkipWhy::NameNotUtf8);
    assert!(survey.traces.is_empty() && survey.dir_errors.is_empty());
}

/// A `*.ndjson` whose name is not UTF-8 is said, never dropped; `scan`
/// still skips it, as before.
#[cfg(unix)]
#[test]
fn a_non_utf8_name_is_said() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = temp_store("survey-name");
    let name = std::ffi::OsStr::from_bytes(b"\xff-journal.ndjson");
    let body = ndjson(&identified("w", 8, Some(EventKind::WorkflowCompleted)));
    if std::fs::write(dir.join(name), &body).is_err() {
        // A filesystem that refuses non-UTF-8 names (APFS) cannot hold one.
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    let survey = survey(&dir);
    assert_eq!(survey.skipped.len(), 1);
    assert_eq!(survey.skipped[0].why, SkipWhy::NameNotUtf8);
    assert!(scan(&dir).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
