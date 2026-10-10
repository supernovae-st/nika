// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! How a replay trial ends for its candidate, read from the runtime's records alone: a step it
//! exercised fails it, a step kept out of it or refused by its room never does, and a filter
//! that kept nothing of the items it read fails it with what came in.

use std::collections::BTreeMap;

use nika_runtime::{RunOutcome, TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause};
use serde_json::{Value, json};

use super::super::ReplayScreen;
use super::{KEPT_NOTHING, Settled, settle, settle_into};
use crate::rehearsal::room::OUTSIDE;

/// The news trial's tasks, in order.
const ORDER: [&str; 4] = ["feed", "filter", "summarize", "write_digest"];

/// The news trial's screen: the model step kept out, the filter reading the feed.
fn news() -> ReplayScreen {
    let upstream = BTreeMap::from([
        ("feed".to_owned(), Vec::new()),
        ("filter".to_owned(), vec!["feed".to_owned()]),
        ("summarize".to_owned(), vec!["filter".to_owned()]),
        ("write_digest".to_owned(), vec!["summarize".to_owned()]),
    ]);
    let kept_out = vec![("summarize".to_owned(), "the model step".to_owned())];
    ReplayScreen::new(kept_out, upstream, vec!["filter".to_owned()])
}

fn order() -> Vec<String> {
    ORDER.iter().map(|task| (*task).to_owned()).collect()
}

fn succeeded(output: Value) -> TaskRecord {
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
    record.output = output;
    record
}

fn failed(code: &str, message: &str) -> TaskRecord {
    let mut record = TaskRecord::unran(TaskStatus::Failure, TerminalCause::VerbError);
    record.error = Some(TaskErrorRecord::new(code, message, false));
    record
}

fn cancelled() -> TaskRecord {
    TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Upstream)
}

fn ran(ok: bool, records: Vec<(&str, TaskRecord)>) -> RunOutcome {
    let records = (records.into_iter())
        .map(|(task, record)| (task.to_owned(), record))
        .collect();
    RunOutcome::new(ok, records, BTreeMap::new())
}

/// A feed of two stories, dated without an offset.
fn feed() -> Value {
    json!({"hits": [
        {"title": "A", "created_at": "2026-10-10T08:00:00"},
        {"title": "B", "created_at": "2026-10-10T09:00:00"}
    ]})
}

#[test]
fn a_step_the_trial_exercised_fails_the_candidate_with_its_facts() {
    let message = "date \"2026-10-10T08:00:00\" does not match format \"%Y-%m-%dT%H:%M:%SZ\"";
    let outcome = ran(
        false,
        vec![
            ("feed", succeeded(feed())),
            ("filter", failed("NIKA-BUILTIN-JQ-002", message)),
            ("summarize", cancelled()),
            ("write_digest", cancelled()),
        ],
    );
    let expected = Settled::Failed {
        task: "filter".to_owned(),
        code: "NIKA-BUILTIN-JQ-002".to_owned(),
        message: message.to_owned(),
    };
    assert_eq!(settle(&outcome, &order(), &news()), expected);
}

#[test]
fn a_step_kept_out_of_the_trial_or_refused_by_its_room_never_fails_it() {
    let refused = format!("request failed: unsupported: {OUTSIDE} · a request was refused");
    let outcome = ran(
        false,
        vec![
            ("feed", succeeded(feed())),
            ("filter", succeeded(json!([{"title": "A"}]))),
            (
                "summarize",
                failed("NIKA-PROVIDER-001", "no API key for 'mock'"),
            ),
            ("write_digest", cancelled()),
        ],
    );
    assert_eq!(settle(&outcome, &order(), &news()), Settled::Passed);
    let elsewhere = ran(
        false,
        vec![
            ("feed", failed("NIKA-BUILTIN-FETCH-001", &refused)),
            ("filter", cancelled()),
            ("summarize", cancelled()),
            ("write_digest", cancelled()),
        ],
    );
    assert_eq!(settle(&elsewhere, &order(), &news()), Settled::Passed);
}

#[test]
fn a_filter_that_kept_nothing_of_its_items_fails_with_what_came_in() -> Result<(), String> {
    let outcome = ran(
        false,
        vec![
            ("feed", succeeded(feed())),
            ("filter", succeeded(json!([]))),
            (
                "summarize",
                failed("NIKA-PROVIDER-001", "no API key for 'mock'"),
            ),
            ("write_digest", cancelled()),
        ],
    );
    let Settled::Failed {
        task,
        code,
        message,
    } = settle(&outcome, &order(), &news())
    else {
        return Err("a filter that kept nothing fails the trial".into());
    };
    assert_eq!((task.as_str(), code.as_str()), ("filter", KEPT_NOTHING));
    assert!(
        message.starts_with("filter kept nothing: 2 items in from feed.hits, 0 out"),
        "{message}"
    );
    assert!(
        message.contains("created_at: string (2026-10-10T08:00:00)"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_filter_behind_a_step_kept_out_of_the_trial_is_not_judged_empty() {
    let mut screen = news();
    screen.filters = vec!["write_digest".to_owned()];
    let outcome = ran(
        true,
        vec![
            ("feed", succeeded(feed())),
            ("filter", succeeded(json!([{"title": "A"}]))),
            ("summarize", succeeded(json!("résumé"))),
            ("write_digest", succeeded(Value::Null)),
        ],
    );
    assert_eq!(settle(&outcome, &order(), &screen), Settled::Passed);
}

#[test]
fn a_run_that_failed_with_no_task_record_fails_the_candidate() -> Result<(), String> {
    let Settled::Failed { task, message, .. } = settle(&ran(false, Vec::new()), &order(), &news())
    else {
        return Err("a failed run with no record fails the trial".into());
    };
    assert!(task.is_empty());
    assert_eq!(message, "the run failed, and no task recorded a failure");
    Ok(())
}

#[test]
fn the_trial_records_a_step_it_kept_out_as_skipped_by_its_error() {
    let mut outcome = ran(
        false,
        vec![
            ("feed", succeeded(feed())),
            ("filter", succeeded(json!([{"title": "A"}]))),
            (
                "summarize",
                failed("NIKA-PROVIDER-001", "no API key for 'mock'"),
            ),
            ("write_digest", cancelled()),
        ],
    );
    settle_into(&mut outcome, &order(), &news());
    assert!(outcome.ok, "no step the trial exercised failed");
    let summarize = &outcome.records["summarize"];
    assert_eq!(summarize.status, TaskStatus::Skipped);
    assert_eq!(summarize.cause, TerminalCause::ErrorSkip);
    assert!(summarize.error.is_some(), "its error is kept");
    assert_eq!(
        outcome.records["write_digest"].status,
        TaskStatus::Cancelled
    );
}

#[test]
fn the_trial_records_a_filter_that_kept_nothing_as_failed() {
    let mut outcome = ran(
        false,
        vec![
            ("feed", succeeded(feed())),
            ("filter", succeeded(json!([]))),
            (
                "summarize",
                failed("NIKA-PROVIDER-001", "no API key for 'mock'"),
            ),
            ("write_digest", cancelled()),
        ],
    );
    settle_into(&mut outcome, &order(), &news());
    assert!(!outcome.ok);
    let filter = &outcome.records["filter"];
    assert_eq!(filter.status, TaskStatus::Failure);
    let code = filter.error.as_ref().map(|error| error.code.as_str());
    assert_eq!(code, Some(KEPT_NOTHING));
}

#[test]
fn the_trial_keeps_a_failure_it_exercised_and_a_run_that_failed_alone() {
    let mut outcome = ran(
        false,
        vec![
            ("feed", succeeded(feed())),
            (
                "filter",
                failed("NIKA-BUILTIN-JQ-002", "does not match format"),
            ),
            ("summarize", cancelled()),
            ("write_digest", cancelled()),
        ],
    );
    settle_into(&mut outcome, &order(), &news());
    assert!(!outcome.ok);
    assert_eq!(outcome.records["filter"].status, TaskStatus::Failure);
    let mut alone = ran(false, Vec::new());
    settle_into(&mut alone, &order(), &news());
    assert!(
        !alone.ok,
        "a run that failed with no task record stays failed"
    );
}
