// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The last observed run kept in HOME history: found again after a reopen as
//! evidence (no workflow authority, no run), an unreadable value kept byte for
//! byte, a record without it unchanged, and a present `null` refused with the
//! journal left as it was.

use super::*;
use crate::run_view::KeptRun;

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";

/// A first session that observed one run with its identity, then closed.
fn closed_after_a_run(root: &Path, home: &Path) {
    let (mut first, _) = open(root, &[ANSWER]);
    first.enable_history(home).expect("fresh history");
    let _ = first.turn(GOAL);
    first.last_workflow = Some(PathBuf::from("two.nika"));
    let mut leg = KeptRun::new();
    leg.execution = Some(EXEC.to_owned());
    leg.workflow_sha256 = Some("ab".repeat(32));
    leg.chain_head = Some("cd".repeat(32));
    leg.chain_len = Some(9);
    let _ = first.observe_run_leg(0, Some(Path::new(".nika/traces/t.ndjson")), leg);
    drop(first);
}

#[test]
fn the_last_run_is_found_again_as_evidence_after_a_reopen() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    closed_after_a_run(root.path(), home.path());
    let (mut later, _) = open(root.path(), &[]);
    later.enable_history(home.path()).expect("resume");
    let kept = later.kept_run().expect("kept").expect("readable");
    assert_eq!(kept.execution.as_deref(), Some(EXEC));
    assert_eq!(kept.workflow.as_deref(), Some("two.nika"));
    assert_eq!((kept.exit, kept.chain_len), (Some(0), Some(9)));
    assert_eq!(kept.trace.as_deref(), Some(".nika/traces/t.ndjson"));
    assert!(
        later.last_workflow.is_none(),
        "evidence, never the workflow to run"
    );
    assert!(later.kept_turns().iter().any(|(user, _)| user == GOAL));
    let legacy = KeptRun::new().ended(None, 1, None);
    let _ = later.observe_run(1, None);
    assert_eq!(
        later.kept_run(),
        Some(Ok(legacy)),
        "the plain door keeps no identity"
    );
}

#[test]
fn an_unreadable_last_run_rides_on_byte_for_byte_and_old_records_have_none() {
    let saved = serde_json::to_value(crate::runtime::history::Saved::default()).expect("saved");
    assert!(saved.get("last_run").is_none(), "no run, no new bytes");
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path(), &[ANSWER, ANSWER]);
    first.enable_history(home.path()).expect("fresh history");
    let newer = serde_json::json!({"version": 2, "next": "run it"});
    first.kept_run = Some(newer.clone());
    let _ = first.turn(GOAL);
    drop(first);
    let (mut later, _) = open(root.path(), &[]);
    later.enable_history(home.path()).expect("resume");
    assert!(matches!(later.kept_run(), Some(Err(why)) if why.contains("version 2")));
    assert_eq!(later.kept_run.as_ref(), Some(&newer), "never normalized");
}

#[test]
fn a_present_null_last_run_is_refused_and_the_journal_kept() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    closed_after_a_run(root.path(), home.path());
    let log = history_dir(home.path(), root.path()).join("events.ndjson");
    let text = std::fs::read_to_string(&log).expect("journal");
    let last = text.lines().last().expect("a record");
    let mut record: serde_json::Value = serde_json::from_str(last).expect("record");
    let state = find_state(&mut record).expect("the saved state");
    state["last_run"] = serde_json::Value::Null;
    let edited = text.replacen(last, &record.to_string(), 1);
    std::fs::write(&log, &edited).expect("edit");
    let (mut later, _) = open(root.path(), &[]);
    let refused = later.enable_history(home.path()).expect_err("refused");
    assert!(refused.text.contains("present but null"), "{refused:?}");
    assert_eq!(
        std::fs::read_to_string(&log).expect("journal"),
        edited,
        "kept as it was"
    );
}

/// The object holding the saved conversation (`recent` is its own key).
fn find_state(value: &mut serde_json::Value) -> Option<&mut serde_json::Value> {
    if value.get("recent").is_some() {
        return Some(value);
    }
    match value {
        serde_json::Value::Object(map) => map.values_mut().find_map(find_state),
        serde_json::Value::Array(items) => items.iter_mut().find_map(find_state),
        _ => None,
    }
}
