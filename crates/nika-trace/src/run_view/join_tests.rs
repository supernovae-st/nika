// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A kept run's execution joined to a saved component's tasks: the execution's own start and
//! its own frames, never the last start, the exit or a task name read across executions, and
//! unknown where the frames cannot attribute.

use super::*;

const A: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const B: &str = "01a0ef11-0212-70de-a8b3-99de94270000";
const SAVED: &str = "aa";
const OTHER: &str = "bb";
const COMPONENT: [&str; 2] = ["log_line", "write_line"];

fn frame(exec: &str, kind: &str, fields: &str) -> String {
    format!(
        r#"{{"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"}},"kind":"{kind}","run":null,"timestamp":1}}"#
    )
}

fn start(exec: &str, hash: &str) -> String {
    frame(
        exec,
        "workflow_started",
        &format!(
            r#"{{"key":"workflow","value":"w"}},{{"key":"workflow_sha256","value":"{hash}"}}"#
        ),
    )
}

fn task(exec: &str, kind: &str, id: &str) -> String {
    frame(exec, kind, &format!(r#"{{"key":"task","value":"{id}"}}"#))
}

fn ended(exec: &str, kind: &str, status: &str) -> String {
    frame(
        exec,
        kind,
        &format!(r#"{{"key":"status","value":"{status}"}}"#),
    )
}

fn kept(exec: &str) -> KeptRun {
    let mut kept = KeptRun::new();
    kept.execution = Some(exec.to_owned());
    kept
}

/// Each component task with how the joined execution settles it.
fn settled(run: &ComponentRun) -> Vec<(&str, &str)> {
    match run {
        ComponentRun::Joined(tasks) => tasks.iter().map(|t| (t.id(), t.settled())).collect(),
        other => panic!("not joined: {other:?}"),
    }
}

/// A root-only run of the saved bytes: the component's tasks completed in that execution, and a
/// task of the root outside the component changes nothing.
#[test]
fn the_executions_own_start_and_tasks_join_the_component() {
    let journal = [
        start(A, SAVED),
        task(A, "task_started", "prepare"),
        task(A, "task_completed", "prepare"),
        task(A, "task_started", "log_line"),
        task(A, "task_completed", "log_line"),
        task(A, "task_started", "write_line"),
        task(A, "task_completed", "write_line"),
        ended(A, "workflow_completed", "succeeded"),
    ]
    .join("\n");
    let run = kept(A).component_run(&journal, SAVED, &COMPONENT);
    assert_eq!(
        settled(&run),
        [("log_line", "completed"), ("write_line", "completed")]
    );
    assert!(run.ran());
}

/// A run that succeeded did not run a component task it skipped, recovered or replayed from a
/// cache, nor one no frame of it names; a failed task is said failed. The terminal word never
/// stands in for the task's own frames.
#[test]
fn a_succeeded_run_does_not_run_a_task_it_skipped_recovered_or_replayed() {
    for (kind, word) in [
        ("task_skipped", "skipped"),
        ("task_recovered", "recovered"),
        ("task_cache_hit", "cache_hit"),
    ] {
        let journal = [
            start(A, SAVED),
            task(A, "task_completed", "log_line"),
            task(A, kind, "write_line"),
            ended(A, "workflow_completed", "succeeded"),
        ]
        .join("\n");
        let facts = RunFacts::of(Path::new("t.ndjson"), &journal).expect("frames");
        assert_eq!(facts.terminal(), Some("succeeded"));
        let run = kept(A).component_run(&journal, SAVED, &COMPONENT);
        assert_eq!(
            settled(&run),
            [("log_line", "completed"), ("write_line", word)],
            "{kind}"
        );
        assert!(!run.ran(), "{kind}");
    }
    let unnamed = [
        start(A, SAVED),
        task(A, "task_completed", "log_line"),
        ended(A, "workflow_completed", "succeeded"),
    ]
    .join("\n");
    let run = kept(A).component_run(&unnamed, SAVED, &COMPONENT);
    assert_eq!(
        settled(&run),
        [("log_line", "completed"), ("write_line", "absent")]
    );
    assert!(!run.ran());
    let failed = [
        start(A, SAVED),
        task(A, "task_completed", "log_line"),
        task(A, "task_failed", "write_line"),
        ended(A, "workflow_failed", "failed"),
    ]
    .join("\n");
    let run = kept(A).component_run(&failed, SAVED, &COMPONENT);
    assert_eq!(
        settled(&run),
        [("log_line", "completed"), ("write_line", "failed")]
    );
    assert!(!run.ran());
}

/// Another execution's task of the same name is never this one's: the root's own frames settle
/// it, where the result view's fold by name reads the child's.
#[test]
fn another_executions_task_of_the_same_name_is_never_this_ones() {
    let journal = [
        start(A, SAVED),
        task(A, "task_completed", "log_line"),
        task(A, "task_skipped", "write_line"),
        start(B, OTHER),
        task(B, "task_started", "write_line"),
        task(B, "task_completed", "write_line"),
        ended(B, "workflow_completed", "succeeded"),
        ended(A, "workflow_completed", "succeeded"),
    ]
    .join("\n");
    let by_name = RunFacts::of(Path::new("t.ndjson"), &journal).expect("frames");
    let folded = (by_name.tasks.iter())
        .find(|t| t.id == "write_line")
        .expect("folded by name");
    assert_eq!(
        folded.state,
        TaskState::Ok,
        "the fold by name reads the child's frames"
    );
    let run = kept(A).component_run(&journal, SAVED, &COMPONENT);
    assert_eq!(
        settled(&run),
        [("log_line", "completed"), ("write_line", "skipped")]
    );
    assert!(!run.ran());
    assert_eq!(
        kept(B).component_run(&journal, SAVED, &COMPONENT),
        ComponentRun::OtherSource(OTHER.to_owned())
    );
}

/// The key is the execution's own start, never the last start a journal holds: the saved bytes
/// started first join, and a later start of the saved bytes by another execution joins nothing.
#[test]
fn the_last_start_of_a_journal_is_never_the_key() {
    let done = [
        task(A, "task_completed", "log_line"),
        task(A, "task_completed", "write_line"),
    ]
    .join("\n");
    let saved_first = [start(A, SAVED), done.clone(), start(B, OTHER)].join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &saved_first).expect("frames");
    assert_eq!(facts.workflow_sha256(), Some(OTHER));
    assert!(kept(A).component_run(&saved_first, SAVED, &COMPONENT).ran());
    let saved_last = [start(A, OTHER), done, start(B, SAVED)].join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &saved_last).expect("frames");
    assert_eq!(facts.workflow_sha256(), Some(SAVED));
    assert_eq!(
        kept(A).component_run(&saved_last, SAVED, &COMPONENT),
        ComponentRun::OtherSource(OTHER.to_owned())
    );
}

/// What the frames cannot attribute stays unknown, with why: no execution kept, no task asked,
/// no frame of the execution, no start, two starts, a start naming no hash, and a frame with no
/// execution naming a component task. A frame with no execution naming another task is not
/// this join's.
#[test]
fn what_the_frames_cannot_attribute_stays_unknown() {
    let done = [
        task(A, "task_completed", "log_line"),
        task(A, "task_completed", "write_line"),
    ]
    .join("\n");
    let whole = format!("{}\n{done}", start(A, SAVED));
    let joined = |journal: &str| kept(A).component_run(journal, SAVED, &COMPONENT);
    let unknown = |run: ComponentRun, why: &str| match run {
        ComponentRun::Unknown(reason) => assert!(reason.contains(why), "{why}: {reason}"),
        other => panic!("{why}: {other:?}"),
    };
    unknown(
        KeptRun::new().component_run(&whole, SAVED, &COMPONENT),
        "names no execution",
    );
    unknown(
        kept(A).component_run(&whole, SAVED, &[]),
        "no component task",
    );
    unknown(
        kept(B).component_run(&whole, SAVED, &COMPONENT),
        "no frame of execution",
    );
    unknown(joined(&done), "has no start");
    unknown(
        joined(&format!("{}\n{whole}", start(A, SAVED))),
        "started 2 times",
    );
    unknown(
        joined(&format!("{}\n{done}", frame(A, "workflow_started", ""))),
        "names no source hash",
    );
    let anonymous = r#"{"fields":[{"key":"task","value":"write_line"}],"kind":"task_failed"}"#;
    unknown(
        joined(&format!("{whole}\n{anonymous}")),
        "no execution names the task `write_line`",
    );
    let unrelated = r#"{"fields":[{"key":"task","value":"prepare"}],"kind":"task_failed"}"#;
    assert!(joined(&format!("{whole}\n{unrelated}")).ran());
}
