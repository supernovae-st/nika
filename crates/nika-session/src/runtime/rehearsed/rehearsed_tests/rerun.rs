// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rehearsed workflow runs again over its own output: this session's settled run moves the
//! proof past what it completed writing, through the real observation door, and nothing else
//! does. The run is stood in for by the bytes it would write and a trace shaped like a real
//! one (frames only; no provider, no engine).

use super::*;

const TARGET: &str = "./out/copied.txt";

/// The frames of a run of `workflow` bytes that completed a `nika:write` of `TARGET`.
fn trace(root: &Path, workflow_sha256: &str, terminal: &str, completed: bool) -> PathBuf {
    let execution = json!({"uuid": "01a10897-52a2-7758-aa31-024a0098d5b1"});
    let frame =
        |kind: &str, fields: Value| json!({"kind": kind, "execution": execution, "fields": fields});
    let field = |pairs: &[(&str, Value)]| -> Value {
        pairs
            .iter()
            .map(|(k, v)| json!({"key": k, "value": v}))
            .collect()
    };
    let mut frames = vec![
        frame(
            "workflow_started",
            field(&[("workflow_sha256", json!(workflow_sha256))]),
        ),
        frame(
            "task_started",
            field(&[
                ("task", json!("copy")),
                ("note", json!("invoke · nika:write")),
            ]),
        ),
        frame(
            "permit_checked",
            field(&[
                ("task", json!("copy")),
                ("plane", json!("fs")),
                ("gate", json!(format!("permits.fs.write {TARGET}"))),
                ("decision", json!("allow")),
            ]),
        ),
    ];
    if completed {
        // As a run journals a completion: the returned value as JSON, the outcome as an object.
        let payload = json!({"attempts": 1, "value": TARGET});
        let outcome = json!({"cause": "normal", "class": "success", "payload": payload});
        frames.push(frame(
            "task_completed",
            field(&[
                ("task", json!("copy")),
                ("note", json!("invoke · nika:write")),
                ("output", json!(json!(TARGET).to_string())),
                ("outcome", json!(outcome.to_string())),
            ]),
        ));
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
    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    proposal(s.turn(INTENT));
    facts(s.consent("yes"));
    let TurnOutcome::RunRequested { .. } = s.turn(RUN) else {
        panic!("the first run is requested");
    };
    write(root.path(), TARGET, USER);
    let landed = std::fs::read(root.path().join(LANDED)).expect("landed");
    let sha = sha.map_or_else(|| sha256_hex(&landed), str::to_owned);
    let at = trace(root.path(), &sha, terminal, completed);
    let _ = s.observe_run(u8::from(terminal != "succeeded"), Some(&at));
    (root, s)
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
