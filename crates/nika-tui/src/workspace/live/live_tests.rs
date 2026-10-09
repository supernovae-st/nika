// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One leg of a run, as the shell observes it: its execution binds at the
//! first frame and another's never reaches the fold, a repeated event counts
//! once, the graph is bound only to the bytes the run names, and the
//! settlement, the stream's wholeness and the declared evidence stay apart.

use super::*;
use crate::session::acquire::Proven;
use nika_display::run_story::RunFrame;

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const OTHER: &str = "01a0ef11-0212-70de-a8b3-99de94270000";
const SOURCE: &str = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n  second:\n    with: { x: \"${{ tasks.first.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: two } }\n";

/// A runtime event of `kind` in execution `exec`, with id `n` and `fields`.
fn event(exec: &str, n: u32, kind: &str, fields: &str) -> RunFrame {
    let line = format!(
        r#"{{"correlation":null,"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
    );
    RunFrame::decode(&line).expect("a runtime event")
}

fn task(exec: &str, n: u32, kind: &str, id: &str) -> RunFrame {
    event(
        exec,
        n,
        kind,
        &format!(r#"{{"key":"task","value":"{id}"}}"#),
    )
}

fn start(exec: &str, bytes: &str) -> RunFrame {
    let hash = nika_event_sha256(bytes);
    event(
        exec,
        1,
        "workflow_started",
        &format!(
            r#"{{"key":"workflow","value":"two"}},{{"key":"workflow_sha256","value":"{hash}"}}"#
        ),
    )
}

/// The sha256 a start names, computed the way the engine does (hex).
fn nika_event_sha256(bytes: &str) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes.as_bytes())
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{b:02x}"));
            hex
        })
}

fn settled(exec: &str, status: &str) -> RunFrame {
    let line = format!(
        r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":2,"execution":{{"uuid":"{exec}"}},"evidence":"unsealed","spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"tasks":{{"cancelled":0,"failed":0,"never_started":0,"ok":2,"recovered":0,"skipped":0,"total":2}},"outputs":{{}}}}"#
    );
    RunFrame::decode(&line).expect("a settlement")
}

fn look() -> Inspected {
    crate::session::judge_for_tests("two.nika", "w".repeat(64), SOURCE)
}

fn text(run: &LiveRun) -> String {
    let (title, body) = run.lines(RunFace::Run, 100, false, false);
    std::iter::once(title)
        .chain(body)
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn before_its_first_frame_a_run_has_no_identity() {
    let run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    assert_eq!(run.execution(), None);
    assert_eq!(run.binding(), Binding::Waiting);
    assert_eq!(run.label(), "run (starting)");
    let pinned = run.pinned("demo");
    assert_eq!(pinned.state, TaskState::Pending);
    assert!(text(&run).contains("no run identity yet"), "{}", text(&run));
    assert!(!run.whole());
}

#[test]
fn a_whole_leg_binds_its_bytes_and_settles_apart_from_its_proof() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 3, "task_scheduled", "second"));
    run.apply(task(EXEC, 4, "task_started", "first"));
    assert_eq!(run.binding(), Binding::Same);
    assert_eq!(run.task("first"), Some(TaskState::Running));
    assert_eq!(run.task("second"), Some(TaskState::Pending));
    assert!(text(&run).contains("running"), "{}", text(&run));
    run.apply(task(EXEC, 5, "task_completed", "first"));
    run.apply(task(EXEC, 6, "task_started", "second"));
    run.apply(task(EXEC, 7, "task_completed", "second"));
    run.apply(event(
        EXEC,
        8,
        "workflow_completed",
        r#"{"key":"status","value":"succeeded"}"#,
    ));
    assert!(
        !run.whole(),
        "the runtime's own end frame is not a settlement"
    );
    assert!(
        text(&run).contains("no settlement received"),
        "{}",
        text(&run)
    );
    run.apply(settled(EXEC, "succeeded"));
    assert!(run.whole());
    assert_eq!(run.reported(), Some(RunState::Succeeded));
    let shown = text(&run);
    // The state has one home, the header row; the settlement row says the rest.
    assert!(
        shown.contains("two.nika · a fresh run · settled · succeeded"),
        "{shown}"
    );
    assert!(
        shown.contains("settlement · normal · 2/2 tasks ok · 2 ms"),
        "{shown}"
    );
    assert_eq!(shown.matches("settled · ").count(), 1, "{shown}");
    assert!(
        shown.contains("evidence · unsealed, as the run declared it"),
        "{shown}"
    );
    assert!(
        shown.contains("first") && shown.contains("second"),
        "{shown}"
    );
    let hex: String = EXEC.replace('-', "").chars().take(12).collect();
    assert_eq!(run.label(), format!("run {hex}"));
}

#[test]
fn the_ascii_run_face_says_its_state_in_the_glyph_column_in_use() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(settled(EXEC, "failed"));
    let (_, body) = run.lines(RunFace::Run, 100, true, false);
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    // The header row is the state's one home: under --ascii it keeps it.
    let header = "two.nika - a fresh run - settled - failed";
    assert!(rows.iter().any(|row| row == header), "{rows:#?}");
    assert!(
        !rows.iter().any(|row| row.contains("settled ·")),
        "{rows:#?}"
    );
}

#[test]
fn a_frame_of_another_execution_never_reaches_the_fold() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(OTHER, 3, "task_completed", "first"));
    run.apply(settled(OTHER, "succeeded"));
    assert_eq!(run.task("first"), Some(TaskState::Pending), "untouched");
    assert_eq!(run.reported(), None, "a foreign settlement closes nothing");
    run.apply(settled(EXEC, "failed"));
    assert_eq!(run.reported(), Some(RunState::Failed));
    assert!(
        !run.whole(),
        "the set-aside frames keep the stream incomplete"
    );
    assert!(
        text(&run).contains("2 of another run, set aside"),
        "{}",
        text(&run)
    );
}

#[test]
fn a_repeated_event_counts_once_and_a_second_settlement_is_not_applied() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 4, "task_started", "first"));
    run.apply(task(EXEC, 5, "task_completed", "first"));
    run.apply(task(EXEC, 5, "task_completed", "first"));
    run.apply(settled(EXEC, "succeeded"));
    run.apply(settled(EXEC, "failed"));
    assert_eq!(
        run.reported(),
        Some(RunState::Succeeded),
        "the first settles"
    );
    assert_eq!(run.task("first"), Some(TaskState::Ok));
    assert!(text(&run).contains("4 events"), "{}", text(&run));
}

#[test]
fn other_bytes_or_an_unseen_start_leave_the_graph_unbound() {
    let mut changed = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    changed.apply(start(EXEC, "nika: two\n# edited after the look\n"));
    assert_eq!(changed.binding(), Binding::Other);
    assert!(text(&changed).contains("graph not bound · the run names other bytes"));
    let mut late = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    late.apply(task(EXEC, 2, "task_scheduled", "first"));
    assert_eq!(late.binding(), Binding::Unseen);
    let mut unread = LiveRun::asked("two.nika".to_owned(), false, true, None);
    unread.apply(start(EXEC, SOURCE));
    assert_eq!(unread.binding(), Binding::Unnamed);
}

#[test]
fn what_was_lost_on_the_way_stays_visible_after_the_settlement() {
    let mut run = LiveRun::asked("two.nika".to_owned(), true, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(settled(EXEC, "succeeded"));
    run.lost(4, 1);
    assert!(!run.whole());
    let shown = text(&run);
    assert!(
        shown.contains("4 lost on the way") && shown.contains("1 unread"),
        "{shown}"
    );
    assert!(shown.contains("a resumed leg"), "{shown}");
}

#[test]
fn the_bound_graph_paints_each_node_in_its_state() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 3, "task_scheduled", "second"));
    run.apply(task(EXEC, 4, "task_started", "first"));
    run.apply(task(EXEC, 5, "task_completed", "first"));
    let shown = text(&run);
    assert!(shown.contains("✔ first"), "{shown}");
    assert!(shown.contains("○ second"), "{shown}");
    let (_, body) = run.lines(RunFace::Run, 100, true, false);
    let ascii: String = body.iter().map(ToString::to_string).collect();
    assert!(
        ascii.contains("okfirst") || ascii.contains("ok first"),
        "{ascii}"
    );
}

#[test]
fn a_frame_after_the_settlement_is_set_aside_and_leaves_the_stream_incomplete() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 3, "task_started", "first"));
    run.apply(task(EXEC, 4, "task_completed", "first"));
    run.apply(settled(EXEC, "succeeded"));
    assert!(run.whole());
    run.apply(task(EXEC, 5, "task_failed", "first"));
    assert_eq!(run.task("first"), Some(TaskState::Ok), "never folded");
    assert!(!run.whole());
    let shown = text(&run);
    assert!(
        shown.contains("1 after the settlement, set aside"),
        "{shown}"
    );
    assert!(shown.contains("4 events folded"), "{shown}");
}

#[test]
fn a_task_that_succeeds_without_having_started_is_counted_out_of_order() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 3, "task_completed", "first"));
    run.apply(settled(EXEC, "succeeded"));
    assert_eq!(
        run.task("first"),
        Some(TaskState::Ok),
        "folded as the runtime said"
    );
    assert!(!run.whole());
    assert!(text(&run).contains("1 out of order"), "{}", text(&run));
}

#[test]
fn past_the_bound_no_event_id_is_kept_and_each_event_is_counted() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    let kept = u32::try_from(EVENTS_KEPT).expect("bound");
    for n in 2..=kept {
        run.apply(event(EXEC, n, "permit_checked", ""));
    }
    assert_eq!(
        (run.events, run.seen.len(), run.beyond),
        (EVENTS_KEPT, EVENTS_KEPT, 0)
    );
    for n in kept + 1..kept + 4 {
        run.apply(event(EXEC, n, "permit_checked", ""));
    }
    // An id seen inside the window, again past it: counted beyond, not repeated.
    run.apply(event(EXEC, 2, "permit_checked", ""));
    assert_eq!((run.events, run.seen.len()), (EVENTS_KEPT, EVENTS_KEPT));
    assert_eq!((run.beyond, run.repeated), (4, 0));
    run.apply(settled(EXEC, "succeeded"));
    assert!(!run.whole());
    assert!(
        text(&run).contains("4 past the fold's bound"),
        "{}",
        text(&run)
    );
}

#[test]
fn a_settlement_that_comes_first_leaves_the_start_unseen() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(settled(EXEC, "succeeded"));
    assert_eq!(run.binding(), Binding::Unseen);
    assert!(
        text(&run).contains("the run's start was not observed"),
        "{}",
        text(&run)
    );
}

#[test]
fn a_story_only_runner_leaves_the_leg_unfollowed_and_says_so() {
    let run = LiveRun::asked("two.nika".to_owned(), false, false, Some(look()));
    let shown = text(&run);
    assert!(shown.contains("story only"), "{shown}");
    let typed = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    assert!(!text(&typed).contains("story only"));
}

/// A write task's two frames, its output the path it wrote (JSON-encoded).
fn wrote(exec: &str, n: u32, id: &str, path: &str) -> [RunFrame; 2] {
    let started = format!(
        r#"{{"key":"task","value":"{id}"}},{{"key":"note","value":"invoke · nika:write"}}"#
    );
    let output = serde_json::to_string(&serde_json::to_string(path).expect("json")).expect("json");
    let completed =
        format!(r#"{{"key":"task","value":"{id}"}},{{"key":"output","value":{output}}}"#);
    [
        event(exec, n, "task_started", &started),
        event(exec, n + 1, "task_completed", &completed),
    ]
}

fn face(run: &LiveRun, face: RunFace) -> String {
    let (title, body) = run.lines(face, 100, false, false);
    std::iter::once(title)
        .chain(body)
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn each_face_shows_only_what_was_acquired_and_names_the_rest() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "save"));
    for frame in wrote(EXEC, 3, "save", "./out/copy.md") {
        run.apply(frame);
    }
    assert!(face(&run, RunFace::Outputs).contains("outputs are unknown, never empty"));
    assert!(
        run.wants(RunFace::Proof).is_empty(),
        "no settlement, no journal yet"
    );
    run.apply(settled(EXEC, "succeeded"));
    assert_eq!(
        run.wants(RunFace::Files),
        [Want::File("./out/copy.md".to_owned())]
    );
    assert_eq!(run.wants(RunFace::Proof), [Want::Proof]);
    assert!(run.wants(RunFace::Run).is_empty() && run.wants(RunFace::Outputs).is_empty());
    let files = face(&run, RunFace::Files);
    assert!(
        files.contains("reported written by `save`") && files.contains("not read yet"),
        "{files}"
    );
    assert!(face(&run, RunFace::Proof).contains("captured and verified when this face opens"));
    run.fetched(crate::session::acquire::Fetched::refused(
        "./out/copy.md",
        "a symlink",
    ));
    assert!(run.wants(RunFace::Files).is_empty());
    assert!(face(&run, RunFace::Files).contains("not read · a symlink"));
    let doc = serde_json::json!({"tier": "ok", "exit": 0, "chain": {"events": 5, "head": "cd".repeat(32), "headline": "intact"}, "lines": ["UNSEALED — no run_sealed frame"]});
    run.proven(Proven::judged(
        ".nika/traces/t.ndjson",
        doc.clone(),
        Vec::new(),
    ));
    let proof = face(&run, RunFace::Proof);
    assert!(
        proof.contains("verdict · OK · exit 0")
            && proof.contains("this run's journal · its execution, source and receipt match"),
        "{proof}"
    );
    assert!(
        proof.contains("UNSEALED — no run_sealed frame")
            && proof.contains("never proves the work was right"),
        "{proof}"
    );
    run.proven(Proven::judged(
        ".nika/traces/t.ndjson",
        doc,
        vec!["the journal records another execution".to_owned()],
    ));
    let foreign = face(&run, RunFace::Proof);
    assert!(
        foreign.contains("not bound to this run · no badge")
            && foreign.contains("another execution"),
        "{foreign}"
    );
    assert!(!foreign.contains("this run's journal"), "{foreign}");
    run.forget();
    assert_eq!(
        run.wants(RunFace::Proof),
        [Want::Proof],
        "read again on demand"
    );
}

/// A leg kept from an earlier session (exit 0), nothing read yet.
fn kept_leg() -> LiveRun {
    let mut kept = nika_session::KeptRun::new();
    kept.workflow = Some("two.nika".to_owned());
    kept.exit = Some(0);
    let execution = serde_json::from_value(serde_json::json!({ "uuid": EXEC })).expect("id");
    LiveRun::kept(kept, execution)
}

/// The Proof of a kept leg's verified journal, lending `frames`' events.
fn lending(frames: Vec<RunFrame>) -> Proven {
    let doc = serde_json::json!({"tier": "ok", "exit": 0, "chain": {"events": 5, "head": "cd".repeat(32), "headline": "intact"}, "lines": []});
    let events = (frames.into_iter())
        .filter_map(|f| match f {
            RunFrame::Event(event) => Some(*event),
            _ => None,
        })
        .collect();
    Proven::judged(".nika/traces/t.ndjson", doc, Vec::new()).lending(events)
}

#[test]
fn a_kept_leg_is_evidence_whose_journal_is_read_first_on_every_face() {
    let run = kept_leg();
    let execution = serde_json::from_value(serde_json::json!({ "uuid": EXEC })).expect("id");
    let shown = face(&run, RunFace::Run);
    assert!(
        shown.contains("earlier session · exit 0") && shown.contains("nothing was replayed"),
        "{shown}"
    );
    assert!(shown.contains("not followed in this session"), "{shown}");
    for each in RunFace::ALL {
        assert_eq!(run.wants(each), [Want::Proof], "{each:?}");
    }
    for each in [RunFace::Run, RunFace::Files, RunFace::Outputs] {
        let said = face(&run, each);
        assert!(
            said.contains(
                "Click this run in the project list to read and verify its saved journal."
            ),
            "{said}"
        );
    }
    assert_eq!(run.execution(), Some(execution));
}

/// Its verified journal's events, its execution's only, become a kept leg's
/// tasks, outputs and written names, named after that journal; a refresh
/// takes them away until the journal is read again.
#[test]
fn a_kept_leg_folds_its_verified_journal_and_names_it() {
    let mut run = kept_leg();
    let mut frames = vec![event(EXEC, 1, "workflow_started", "")];
    frames.extend(wrote(EXEC, 2, "save", "./out/copy.md"));
    frames.push(task(OTHER, 4, "task_started", "foreign"));
    let outputs = r#"{"key":"outputs","value":"{\"total\":5}"}"#;
    frames.push(event(EXEC, 5, "workflow_completed", outputs));
    run.proven(lending(frames));
    let shown = face(&run, RunFace::Run);
    assert!(
        shown.contains("journal .nika/traces/t.ndjson")
            && shown.contains("captured bytes abababababab"),
        "{shown}"
    );
    assert!(
        shown.contains("✔ save") && !shown.contains("foreign"),
        "{shown}"
    );
    let (_, detail) = run.detail("save", nika_tui_view::Canvas::new(100, false, false));
    let detail = (detail.iter().map(ToString::to_string))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        detail.contains("as its journal recorded it") && detail.contains("15 bytes in its journal"),
        "{detail}"
    );
    let outputs = face(&run, RunFace::Outputs);
    assert!(
        outputs.contains("total") && outputs.contains("its journal"),
        "{outputs}"
    );
    assert_eq!(
        run.wants(RunFace::Files),
        [Want::File("./out/copy.md".to_owned())]
    );
    assert!(face(&run, RunFace::Files).contains("reported written by `save`"));
    run.forget();
    assert_eq!(run.wants(RunFace::Files), [Want::Proof], "journal first");
    assert!(
        !face(&run, RunFace::Run)
            .split_whitespace()
            .any(|word| word == "save"),
        "{}",
        face(&run, RunFace::Run)
    );
}

/// A kept leg's outputs face says what its journal's close recorded: the
/// map, an empty map, only a size, a whole withholding, an unreadable
/// record, none (an older close) or no close; a refused journal shows none.
#[test]
fn a_kept_leg_s_outputs_say_what_its_journal_recorded() {
    let said = |fields: &str| {
        let mut run = kept_leg();
        run.proven(lending(vec![event(EXEC, 1, "workflow_completed", fields)]));
        face(&run, RunFace::Outputs)
    };
    let cases = [
        (
            r#"{"key":"outputs","value":"{}"}"#,
            "records an empty outputs map",
        ),
        (
            r#"{"key":"outputs_bytes","value":70000}"#,
            "70000 bytes: only its size",
        ),
        (
            r#"{"key":"outputs_withheld","value":true}"#,
            "withheld whole",
        ),
        (r#"{"key":"outputs","value":"{\"a\":"}"#, "cannot be read"),
        (r#"{"key":"status","value":"succeeded"}"#, "older engine"),
    ];
    for (fields, words) in cases {
        let text = said(fields);
        assert!(text.contains(words), "{fields}: {text}");
        assert!(!text.contains("total"), "{text}");
    }
    let mut open = kept_leg();
    open.proven(lending(vec![event(EXEC, 1, "workflow_started", "")]));
    assert!(face(&open, RunFace::Outputs).contains("holds no terminal frame"));
    let mut refused = kept_leg();
    refused.proven(Proven::refused(".nika/traces/t.ndjson", "a symlink"));
    for each in [RunFace::Run, RunFace::Outputs, RunFace::Files] {
        let text = face(&refused, each);
        assert!(
            text.contains("nothing it records is shown: a symlink"),
            "{text}"
        );
    }
}

#[test]
fn graph_cards_do_not_turn_an_unobserved_task_into_a_scheduled_task() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    let graph = run
        .graph(62, false, true)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(graph.contains("definition"), "{graph}");
    assert!(!graph.contains("scheduled"), "{graph}");
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    let graph = run
        .graph(62, false, true)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(graph.contains("scheduled"), "{graph}");
    assert!(
        graph.contains("definition"),
        "unobserved second task: {graph}"
    );
    let mut changed = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    changed.apply(start(EXEC, "other bytes"));
    assert!(
        !text(&changed).contains("definition"),
        "unbound cards must not be painted"
    );
}
