// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rehearsed workflow runs again over its own output: this session's settled run moves the
//! proof past what it completed writing, through the real observation door, and nothing else
//! does. The run is stood in for by the bytes it would write and a trace shaped like a real
//! one (frames only; no provider, no engine).

use super::*;

const TARGET: &str = "./out/copied.txt";

/// The frames of a run of `workflow` bytes whose tasks each wrote one path, `(task, path)`.
fn trace_writing(
    root: &Path,
    workflow_sha256: &str,
    (terminal, completed): (&str, bool),
    written: &[(&str, &str)],
) -> PathBuf {
    let execution = json!({"uuid": "01a10897-52a2-7758-aa31-024a0098d5b1"});
    let frame =
        |kind: &str, fields: Value| json!({"kind": kind, "execution": execution, "fields": fields});
    let field = |pairs: &[(&str, Value)]| -> Value {
        pairs
            .iter()
            .map(|(k, v)| json!({"key": k, "value": v}))
            .collect()
    };
    let mut frames = vec![frame(
        "workflow_started",
        field(&[("workflow_sha256", json!(workflow_sha256))]),
    )];
    for (task, path) in written {
        frames.push(frame(
            "task_started",
            field(&[
                ("task", json!(task)),
                ("note", json!("invoke · nika:write")),
            ]),
        ));
        frames.push(frame(
            "permit_checked",
            field(&[
                ("task", json!(task)),
                ("plane", json!("fs")),
                ("gate", json!(format!("permits.fs.write {path}"))),
                ("decision", json!("allow")),
            ]),
        ));
        if completed {
            // As a run journals a completion: the returned value as JSON, the outcome an object.
            let payload = json!({"attempts": 1, "value": path});
            let outcome = json!({"cause": "normal", "class": "success", "payload": payload});
            frames.push(frame(
                "task_completed",
                field(&[
                    ("task", json!(task)),
                    ("note", json!("invoke · nika:write")),
                    ("output", json!(json!(path).to_string())),
                    ("outcome", json!(outcome.to_string())),
                ]),
            ));
        }
    }
    let end = if terminal == "succeeded" {
        "workflow_completed"
    } else {
        "workflow_failed"
    };
    frames.push(frame(end, field(&[("status", json!(terminal))])));
    let lines: Vec<String> = frames.iter().map(Value::to_string).collect();
    let text = lines.join("\n") + "\n";
    let at = root.join(".nika/traces/run.ndjson");
    std::fs::create_dir_all(at.parent().expect("traces")).expect("dirs");
    std::fs::write(&at, text).expect("trace");
    at
}

/// A landed rehearsed copy run once: the output written as the run would, then observed.
fn ran_once(
    terminal: &str,
    completed: bool,
    sha: Option<&str>,
) -> (tempfile::TempDir, SessionRuntime) {
    ran_writing(terminal, completed, sha, &[])
}

/// [`ran_once`], its run also writing each `(task, path)` of `also` (bytes `[]`), beside `TARGET`.
fn ran_writing(
    terminal: &str,
    completed: bool,
    sha: Option<&str>,
    also: &[(&str, &str)],
) -> (tempfile::TempDir, SessionRuntime) {
    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    proposal(s.turn(INTENT));
    facts(s.consent("yes"));
    let TurnOutcome::RunRequested { .. } = s.turn(RUN) else {
        panic!("the first run is requested");
    };
    write(root.path(), TARGET, USER);
    for (_, path) in also {
        write(root.path(), path, "[]");
    }
    let landed = std::fs::read(root.path().join(LANDED)).expect("landed");
    let sha = sha.map_or_else(|| sha256_hex(&landed), str::to_owned);
    let written: Vec<(&str, &str)> = std::iter::once(("copy", TARGET))
        .chain(also.iter().copied())
        .collect();
    let at = trace_writing(root.path(), &sha, (terminal, completed), &written);
    let _ = s.observe_run(u8::from(terminal != "succeeded"), Some(&at));
    (root, s)
}

/// A run that also writes a state file the request never named is admitted again: that file is
/// no path of the rehearsed world, and the destination it named advances (live gauntlet wave 9
/// r11, 2026-10-20: `etat_echecs.json` beside `statut.log`, then « `statut.log` appeared » on every
/// « lance-le »).
#[test]
fn a_run_that_also_writes_an_unnamed_state_file_is_admitted_again() {
    let (_root, mut s) = ran_writing("succeeded", true, None, &[("save_state", "out/etat.json")]);
    let outcome = s.turn(RUN);
    let TurnOutcome::RunRequested { run, .. } = outcome else {
        panic!("the second run is requested over its own settled outputs: {outcome:?}");
    };
    assert_eq!(run.workflow, PathBuf::from(LANDED));
}

#[test]
fn a_second_run_is_admitted_over_its_own_settled_output() {
    let (_root, mut s) = ran_once("succeeded", true, None);
    let TurnOutcome::RunRequested { run, .. } = s.turn(RUN) else {
        panic!("the second run is requested over the first run's own output");
    };
    assert_eq!(run.workflow, PathBuf::from(LANDED));
}

#[test]
fn restoring_the_exact_source_allows_a_fresh_run_without_recreating_the_workflow() {
    let (root, mut session) = ran_once("succeeded", true, None);
    let source = std::fs::read(root.path().join(SOURCE)).expect("original source");
    let workflow = std::fs::read(root.path().join(LANDED)).expect("saved workflow");
    let sha = sha256_hex(&workflow);
    std::fs::remove_file(root.path().join(SOURCE)).expect("missing input");
    let why = refused(session.turn(RUN));
    assert!(
        why.text.contains("gone"),
        "the refusal names the gone input"
    );
    write(root.path(), SOURCE, EDITED);
    let changed = refused(session.turn(RUN));
    assert!(
        changed.text.contains("changed"),
        "the refusal names the changed input"
    );
    std::fs::write(root.path().join(SOURCE), source).expect("restore exact input");
    assert!(
        session
            .advance_rehearsal(Path::new(LANDED), &sha, &[TARGET.into()])
            .is_err(),
        "a late observation cannot revive the withdrawn proof"
    );
    let outcome = session.turn(RUN);
    let TurnOutcome::RunRequested { run, .. } = outcome else {
        panic!("a fresh request revalidates the restored world: {outcome:?}");
    };
    assert_eq!(run.workflow, PathBuf::from(LANDED));
    assert_eq!(
        std::fs::read(root.path().join(LANDED)).expect("unchanged workflow"),
        workflow
    );
    assert!(
        session.pending_proposal().is_none(),
        "no new proposal or repeated consent"
    );
}

#[test]
fn a_foreign_change_after_the_run_still_withdraws_the_rehearsal() {
    let (root, mut s) = ran_once("succeeded", true, None);
    write(root.path(), TARGET, EDITED);
    let why = refused(s.turn(RUN));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(
        why.text.contains("changed"),
        "the output edited by hand: {}",
        why.text
    );

    let (root, mut s) = ran_once("succeeded", true, None);
    write(root.path(), SOURCE, EDITED);
    let why = refused(s.turn(RUN));
    assert!(
        why.text.contains("was withdrawn"),
        "a source is never advanced: {}",
        why.text
    );
}

#[test]
fn only_this_sessions_completed_write_of_the_rehearsed_bytes_advances() {
    let cases = [
        (
            "a permit without a completed write",
            "succeeded",
            false,
            None,
        ),
        ("a failed run", "failed", true, None),
        (
            "another program's run",
            "succeeded",
            true,
            Some("0".repeat(64)),
        ),
    ];
    for (case, terminal, completed, sha) in cases {
        let (_root, mut s) = ran_once(terminal, completed, sha.as_deref());
        let why = refused(s.turn(RUN));
        assert_eq!(
            why.class,
            RefusalClass::StaleRevision,
            "{case}: {}",
            why.text
        );
        assert!(why.text.contains("appeared"), "{case}: {}", why.text);
    }
}
