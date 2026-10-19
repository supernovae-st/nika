// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The skipped path, tried by a real isolated jq: this test binary serves as the helper, as
//! `nika __nika-jq-eval` does. A filter fails its rehearsal only when its producer's null alone
//! makes it fail, on the run's values and on a day with nothing new.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use nika_runtime::{RunOutcome, TaskRecord, TaskStatus, TerminalCause};
use nika_schema::{FileId, ParseMode};
use serde_json::{Value, json};

use super::{SKIPPED_NULL, try_skipped};
use crate::rehearsal::isolated_jq::{IsolatedJq, JqHelper};

/// The helper's one evaluation, served when the wrapper names this test.
#[test]
#[ignore = "run by the skipped-path tests' jq helper wrapper"]
// The wrapper's switch, read once by this test binary: no secret, no configuration.
#[allow(clippy::disallowed_methods)]
fn jq_helper() {
    if std::env::var_os("NIKA_SKIPPED_PATH_JQ").is_none() {
        return;
    }
    let mut answer = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .unwrap();
    let mut request = std::io::stdin().lock();
    nika_builtin::data::jq_serve(&mut request, &mut answer, 4 * 1024 * 1024).unwrap();
}

/// This binary as the helper, its framed answer on descriptor 3 and libtest's words elsewhere.
fn helper(dir: &Path, within: Duration) -> IsolatedJq {
    let script = dir.join("jq-helper");
    let exe = std::env::current_exe().unwrap();
    let body = format!(
        "#!/bin/sh\nexec /usr/bin/env NIKA_SKIPPED_PATH_JQ=1 '{}' --exact rehearsal::skipped_path::tests::jq_helper --ignored --test-threads=1 -q 3>&1 1>/dev/null 2>/dev/null\n",
        exe.display()
    );
    std::fs::write(&script, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    IsolatedJq::new(JqHelper::new(script), Instant::now() + within)
}

fn parsed(yaml: &str) -> nika_schema::raw::RawWorkflow {
    nika_schema::parse(yaml, FileId::new(0), ParseMode::Strict).unwrap()
}

fn succeeded(output: Value) -> TaskRecord {
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
    record.output = output;
    record
}

fn ran(records: Vec<(&str, TaskRecord)>) -> RunOutcome {
    let records = (records.into_iter())
        .map(|(task, record)| (task.to_owned(), record))
        .collect();
    RunOutcome::new(true, records, BTreeMap::new())
}

/// The outcome after the skipped path was tried on `yaml`, over `records`.
fn tried(yaml: &str, records: Vec<(&str, TaskRecord)>) -> RunOutcome {
    let dir = tempfile::tempdir().unwrap();
    let jq = helper(dir.path(), Duration::from_secs(30));
    let mut outcome = ran(records);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(try_skipped(&parsed(yaml), &mut outcome, &jq));
    outcome
}

/// The live gauntlet r02 shape (2026-10-17): the page fetched again behind a gate, and the
/// version read from it with no `when:` of its own.
fn watch(expression: &str) -> String {
    format!(
        "nika: watch
permits:
  tools: [nika:jq]
tasks:
  changed:
    invoke:
      tool: nika:jq
      args: {{ input: {{}}, expression: 'true' }}
  page_again:
    with: {{ go: '${{{{ tasks.changed.output }}}}' }}
    when: ${{{{ with.go == true }}}}
    invoke:
      tool: nika:jq
      args: {{ input: {{}}, expression: '\"Download Python 3.14.1<\"' }}
  version:
    with: {{ html: '${{{{ tasks.page_again.output }}}}' }}
    invoke:
      tool: nika:jq
      args:
        input: ${{{{ with.html }}}}
        expression: '{expression}'
"
    )
}

/// The day the page was fetched: every step ran.
fn fetched() -> Vec<(&'static str, TaskRecord)> {
    vec![
        ("changed", succeeded(json!(true))),
        ("page_again", succeeded(json!("Download Python 3.14.1<"))),
        ("version", succeeded(json!("3.14.1"))),
    ]
}

/// The r02 filter fails on the null of a day with nothing new, and passes on the page this run
/// fetched: the rehearsal fails there, naming the producer, the null and the repair.
#[test]
fn a_filter_that_fails_on_its_skipped_producers_null_fails_the_rehearsal() {
    let filter = r#"[match("Download Python ([0-9.]+)<"; "g") | .captures[0].string][0]"#;
    let outcome = tried(&watch(filter), fetched());
    assert!(!outcome.ok);
    let record = &outcome.records["version"];
    assert_eq!(record.status, TaskStatus::Failure);
    let error = record.error.as_ref().unwrap();
    assert_eq!(error.code, SKIPPED_NULL);
    assert!(
        error
            .message
            .starts_with("version fails the day page_again is skipped")
            && error.message.contains("null")
            && error.message.contains("`when:`"),
        "{}",
        error.message
    );
}

/// A filter that gives the null a default passes there: nothing is claimed.
#[test]
fn a_filter_that_defaults_the_null_passes() {
    let filter =
        r#"(. // "") | [match("Download Python ([0-9.]+)<"; "g") | .captures[0].string][0]"#;
    let outcome = tried(&watch(filter), fetched());
    assert!(outcome.ok, "{:?}", outcome.records["version"].error);
    assert_eq!(outcome.records["version"].status, TaskStatus::Success);
}

/// The live gauntlet r05 shape: the summaries of a model step behind a count, joined to the
/// journal's other values.
fn journal(expression: &str) -> String {
    format!(
        "nika: journal
permits:
  tools: [nika:jq]
tasks:
  count:
    invoke:
      tool: nika:jq
      args: {{ input: {{}}, expression: '2' }}
  summarize:
    with: {{ n: '${{{{ tasks.count.output }}}}' }}
    when: ${{{{ with.n > 0 }}}}
    invoke:
      tool: nika:jq
      args: {{ input: {{}}, expression: '[\"a\", \"b\"]' }}
  write_lines:
    with:
      n: '${{{{ tasks.count.output }}}}'
      lines: '${{{{ tasks.summarize.output }}}}'
    invoke:
      tool: nika:jq
      args:
        input: {{ n: '${{{{ with.n }}}}', lines: '${{{{ with.lines }}}}' }}
        expression: '{expression}'
"
    )
}

fn summarized() -> Vec<(&'static str, TaskRecord)> {
    vec![
        ("count", succeeded(json!(2))),
        ("summarize", succeeded(json!(["a", "b"]))),
        ("write_lines", succeeded(json!("- a\n- b"))),
    ]
}

/// The r05 filter iterates the summaries: the empty day fails it.
#[test]
fn a_null_inside_an_object_input_fails_the_rehearsal() {
    let outcome = tried(
        &journal(r#"[.lines[] | "- " + .] | join("\n")"#),
        summarized(),
    );
    assert!(!outcome.ok);
    let error = outcome.records["write_lines"].error.as_ref().unwrap();
    assert_eq!(error.code, SKIPPED_NULL);
    assert!(
        error
            .message
            .starts_with("write_lines fails the day summarize is skipped"),
        "{}",
        error.message
    );
}

/// A filter whose own test of another value stands for the skip (the count is zero the day the
/// summaries are skipped) passes on the emptied values: no claim from the run's own count.
#[test]
fn a_filter_guarded_by_a_value_the_empty_day_empties_passes() {
    let filter = r#"if .n > 0 then [.lines[] | "- " + .] | join("\n") else "" end"#;
    let outcome = tried(&journal(filter), summarized());
    assert!(outcome.ok, "{:?}", outcome.records["write_lines"].error);
}

/// A value this run did not give is never guessed: the reader is not tried.
#[test]
fn a_reader_whose_other_value_this_run_did_not_give_is_not_tried() {
    let mut records = summarized();
    records[0] = (
        "count",
        TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Upstream),
    );
    let outcome = tried(&journal(r#"[.lines[] | "- " + .] | join("\n")"#), records);
    assert!(outcome.ok, "{:?}", outcome.records["write_lines"].error);
}

/// With too little time left before the run's deadline, nothing is evaluated or claimed.
#[test]
fn nothing_is_tried_past_the_runs_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let jq = helper(dir.path(), Duration::from_millis(500));
    let filter = r#"[match("Download Python ([0-9.]+)<"; "g") | .captures[0].string][0]"#;
    let mut outcome = ran(fetched());
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(try_skipped(&parsed(&watch(filter)), &mut outcome, &jq));
    assert!(outcome.ok);
}
