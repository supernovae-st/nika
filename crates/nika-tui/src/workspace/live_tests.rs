// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One leg of a run, as the shell observes it: its execution binds at the
//! first frame and another's never reaches the fold, a repeated event counts
//! once, the graph is bound only to the bytes the run names, and the
//! settlement, the stream's wholeness and the declared evidence stay apart.

use nika_display::run_story::RunState;
use nika_display::state::TaskState;

use crate::lang::{Lang, Voice};
use crate::session::acquire::Proven;
use crate::workspace::inspect::Inspected;
use crate::workspace::live::*;
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
    // Named by the second its time-ordered execution began (on a UTC clock here); its hex
    // stays the detail's.
    assert_eq!(run.label(), "run 09-29 21:27:44");
    let hex: String = EXEC.replace('-', "").chars().take(12).collect();
    assert_eq!(run.hex(), Some(hex));
}

/// A run is named on the machine's clock, where the person reads it, never in UTC (live
/// gauntlet 5, section F): four hours east of UTC, the run that began at 07:04:30 UTC is named
/// « 11:04:30 », and one begun at 22:15 UTC is already the next day there. French dates it
/// day/month, English month-day; the hex stays the detail's, whatever the clock.
#[test]
fn a_run_is_named_on_the_machines_clock_and_dated_in_the_persons_language() {
    let east = jiff::tz::TimeZone::fixed(jiff::tz::offset(4));
    let named = |uuid: &str| {
        let mut run = LiveRun::asked("two.nika".to_owned(), false, true, None).zoned(east.clone());
        run.apply(start(uuid, SOURCE));
        (run.label_in(Lang::En), run.label_in(Lang::Fr), run.hex())
    };
    assert_eq!(
        named("01a148ad-2830-70de-b8b3-99de9427fcc1"),
        (
            "run 10-17 11:04:30".to_owned(),
            "run 17/10 11:04:30".to_owned(),
            Some("01a148ad2830".to_owned())
        )
    );
    let (english, french, _) = named("01a146c8-62a0-70de-b8b3-99de9427fcc3");
    assert_eq!(
        (english.as_str(), french.as_str()),
        ("run 10-17 02:15:00", "run 17/10 02:15:00")
    );
    let utc = LiveRun::asked("two.nika".to_owned(), false, true, None);
    assert_eq!(utc.label_in(Lang::Fr), "run (starting)");
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
    let (events, seen, beyond, _) = run.kept_counts();
    assert_eq!((events, seen, beyond), (EVENTS_KEPT, EVENTS_KEPT, 0));
    for n in kept + 1..kept + 4 {
        run.apply(event(EXEC, n, "permit_checked", ""));
    }
    // An id seen inside the window, again past it: counted beyond, not repeated.
    run.apply(event(EXEC, 2, "permit_checked", ""));
    let (events, seen, beyond, repeated) = run.kept_counts();
    assert_eq!((events, seen), (EVENTS_KEPT, EVENTS_KEPT));
    assert_eq!((beyond, repeated), (4, 0));
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
    let (_, detail) = run.detail(
        "save",
        nika_tui_view::Canvas::new(100, false, false),
        crate::lang::Voice::default(),
    );
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
        .graph(62, crate::lang::Voice::default(), true)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(graph.contains("definition"), "{graph}");
    assert!(!graph.contains("scheduled"), "{graph}");
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    let graph = run
        .graph(62, crate::lang::Voice::default(), true)
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

/// No frame outlives the turn that drove a leg: once that turn ended without its settlement,
/// the leg reads ended and a task it saw start reads with no end received, never running, the
/// fold keeping what it saw. A settled leg keeps its settlement; a leg with no frame says none
/// was received.
#[test]
fn a_leg_whose_turn_ended_without_its_settlement_never_reads_running() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 3, "task_started", "first"));
    assert_eq!(run.pinned("demo").state, TaskState::Running);
    let revision = run.revision();
    run.ended();
    assert!(run.revision() > revision, "a new rendering");
    assert_eq!(run.pinned("demo").state, TaskState::Pending);
    let shown = text(&run);
    assert!(shown.contains("ended · no settlement received"), "{shown}");
    assert!(shown.contains("no end received"), "{shown}");
    assert!(!shown.contains("running"), "{shown}");
    assert!(!shown.contains('◐'), "{shown}");
    assert_eq!(
        run.task("first"),
        Some(TaskState::Running),
        "the fold keeps what it saw"
    );
    let mut settled = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    settled.apply(start(EXEC, SOURCE));
    settled.apply(self::settled(EXEC, "succeeded"));
    settled.ended();
    assert_eq!(settled.pinned("demo").state, TaskState::Ok);
    assert!(
        text(&settled).contains("settled · succeeded"),
        "{}",
        text(&settled)
    );
    let mut silent = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    silent.ended();
    let said = text(&silent);
    assert!(said.contains("ended · no frame received"), "{said}");
    assert!(
        said.contains("no frame received before its turn ended"),
        "{said}"
    );
}

/// A settlement of `status` counting `ok` of `total` tasks ok, its evidence `evidence`.
fn tallied(exec: &str, status: &str, (ok, total): (u32, u32), evidence: &str) -> RunFrame {
    let failed = total - ok;
    let line = format!(
        r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":2,"execution":{{"uuid":"{exec}"}},"evidence":"{evidence}","spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"tasks":{{"cancelled":0,"failed":{failed},"never_started":0,"ok":{ok},"recovered":0,"skipped":0,"total":{total}}},"outputs":{{}}}}"#
    );
    RunFrame::decode(&line).expect("a settlement")
}

/// Once the settlement arrives no task runs: one the fold still draws running reads settled ok
/// when the settlement counts every task ok (its own end frame lost on the way), and no end
/// received otherwise, the fold keeping what it saw. A lost journal says why when the stream
/// told it, and `reason not reported` when nothing did.
#[test]
fn a_settled_run_closes_the_tasks_it_still_drew_running() {
    let drained = |status: &str, tally: (u32, u32), evidence: &str| {
        let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
        run.apply(start(EXEC, SOURCE));
        run.apply(task(EXEC, 2, "task_started", "first"));
        run.apply(task(EXEC, 3, "task_completed", "first"));
        run.apply(task(EXEC, 4, "task_started", "second"));
        run.apply(tallied(EXEC, status, tally, evidence));
        run
    };
    let run = drained("succeeded", (2, 2), "lost");
    let shown = text(&run);
    assert!(shown.contains("✔ second · settled ok"), "{shown}");
    assert!(shown.contains("✔ settled ok"), "the card: {shown}");
    assert!(
        !shown.contains('◐') && !shown.contains("running"),
        "{shown}"
    );
    assert_eq!(
        run.task("second"),
        Some(TaskState::Running),
        "the fold keeps it"
    );
    assert!(
        shown.contains("evidence · lost, as the run declared it · reason not reported"),
        "{shown}"
    );
    let failed = drained("failed", (1, 2), "unsealed");
    let shown = text(&failed);
    assert!(shown.contains("○ second · no end received"), "{shown}");
    assert!(!shown.contains("reason not reported"), "{shown}");
    let mut told = drained("succeeded", (2, 2), "lost");
    let why = "a step's output over 1 MiB is not shown, and the run's trace cannot keep it \
               either (1 MiB per line): the run goes on, its trace stops there";
    told.told(why);
    let shown = text(&told).replace('\n', " ");
    assert!(shown.contains("evidence · lost"), "{shown}");
    assert!(shown.contains("its trace stops there"), "{shown}");
    assert!(!shown.contains("reason not reported"), "{shown}");
}

/// The run in view says its own words in the person's language: its faces'
/// names, its standing (on its pinned row too), the facts above its graph (the
/// bytes it binds, how far it went, its settlement, its stream, its evidence),
/// the graph's counts, the chips its tasks wear and the list's keys. The
/// workflow, the cause, the counts and the times keep theirs; under ASCII the
/// French loses its accents; English keeps English.
#[test]
fn the_run_in_view_says_its_own_words_in_french() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "first"));
    run.apply(task(EXEC, 3, "task_scheduled", "second"));
    run.apply(task(EXEC, 4, "task_started", "first"));
    run.apply(task(EXEC, 5, "task_completed", "first"));
    let shown = |run: &LiveRun, voice: Voice| {
        let (title, body) = run.view(RunFace::Run, &Pick::new(), 140, (voice, true), false);
        std::iter::once(title)
            .chain(body)
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    let running = shown(&run, french);
    for said in [
        "[run] sorties fichiers preuve",
        "two.nika · un nouveau run · en cours",
        "graphe · les octets wwwwwwwwwwww sur lesquels il a été demandé, tels que le run les nomme",
        "en cours · 2 tâche(s) vue(s)",
        "flux incomplet · aucune clôture · 5 événements repliés",
        "attestation · pas encore déclarée · la preuve est la vérification propre de la trace",
        "2 tâches · 1 arête · 2 vagues",
        "✔ réussie",
        "planifiée",
        "tâches · ↑↓ choisir · Entrée détails",
    ] {
        assert!(running.contains(said), "{said}\n{running}");
    }
    run.apply(task(EXEC, 6, "task_started", "second"));
    run.apply(task(EXEC, 7, "task_completed", "second"));
    run.apply(settled(EXEC, "succeeded"));
    let settled = shown(&run, french);
    for said in [
        "two.nika · un nouveau run · terminé · réussi",
        "clôture · normal · 2/2 tâches ok · 2 ms",
        "flux · 7 événements et la clôture, entiers",
        "attestation · non scellée, comme le run l'a déclarée · la preuve est la vérification propre de la trace",
    ] {
        assert!(settled.contains(said), "{said}\n{settled}");
    }
    assert_eq!(run.pinned_in("demo", french).words, "terminé · réussi");
    let plain = shown(
        &run,
        Voice {
            lang: Lang::Fr,
            ascii: true,
        },
    );
    assert!(
        plain.contains("two.nika - un nouveau run - termine - reussi"),
        "{plain}"
    );
    assert!(
        plain.contains("taches - Up/Down choisir - Entree details"),
        "{plain}"
    );
    assert!(plain.is_ascii(), "{plain}");
    let english = shown(&run, Voice::default());
    assert!(english.contains("[run] outputs files proof"), "{english}");
    assert!(
        english.contains("two.nika · a fresh run · settled · succeeded"),
        "{english}"
    );
    assert!(
        english.contains("tasks · ↑↓ pick · Enter details"),
        "{english}"
    );
    assert_eq!(run.pinned("demo").words, "settled · succeeded");
}

/// Two writes the run names in its bytes: one path, then a fan-out of two.
const WRITES: &str = "nika: writes\npermits:\n  fs: { write: [\"./out/*.md\"] }\n  tools: [\"nika:write\"]\ntasks:\n  one:\n    invoke: { tool: \"nika:write\", args: { path: \"./out/one.md\", content: \"a\" } }\n  many:\n    for_each:\n      items: [\"a\", \"b\"]\n    invoke: { tool: \"nika:write\", args: { path: \"./out/${{ item }}.md\", content: \"${{ item }}\" } }\n";

/// A task's terminal frame of `kind` with its `note` and its `output` (one JSON text).
fn ended(n: u32, kind: &str, (id, note): (&str, &str), output: &str) -> RunFrame {
    let fields = serde_json::json!([
        {"key": "task", "value": id},
        {"key": "note", "value": note},
        {"key": "output", "value": output},
    ])
    .to_string();
    let inner = fields.trim_start_matches('[').trim_end_matches(']');
    event(EXEC, n, kind, inner)
}

/// The files face lists what the run wrote (live gauntlet 4, r03 B4, r04 B13: « wrote no
/// file » beside three written files): a write its note names, each path a fan-out of
/// `nika:write` wrote (its note counts items), and on a resumed leg the file a cache hit
/// reports, which the same run wrote before its gate. A path is read only from a write's
/// own output, never guessed.
#[test]
fn the_files_face_lists_every_file_the_run_wrote() {
    let look = crate::session::judge_for_tests("writes.nika", "w".repeat(64), WRITES);
    let mut run = LiveRun::asked("writes.nika".to_owned(), false, true, Some(look.clone()));
    run.apply(start(EXEC, WRITES));
    run.apply(task(EXEC, 2, "task_scheduled", "one"));
    run.apply(task(EXEC, 3, "task_scheduled", "many"));
    run.apply(ended(
        4,
        "task_completed",
        ("one", "invoke · nika:write"),
        "\"./out/one.md\"",
    ));
    let many = "[\"./out/a.md\",\"./out/b.md\"]";
    run.apply(ended(
        5,
        "task_completed",
        ("many", "for_each · 2 items"),
        many,
    ));
    assert_eq!(run.binding(), Binding::Same);
    let written = |run: &LiveRun| {
        (run.written().into_iter())
            .map(|(task, path)| format!("{task} {path}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        written(&run),
        ["one ./out/one.md", "many ./out/a.md", "many ./out/b.md"]
    );
    // The resumed leg's cache hit names no tool: its file is the run's all the same.
    let mut resumed = LiveRun::asked("writes.nika".to_owned(), true, true, Some(look));
    resumed.apply(start(EXEC, WRITES));
    resumed.apply(ended(
        2,
        "task_cache_hit",
        ("one", "cache hit"),
        "\"./out/one.md\"",
    ));
    assert_eq!(written(&resumed), ["one ./out/one.md"]);
    let (_, body) = resumed.lines(RunFace::Files, 100, false, false);
    let shown = body
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(shown.contains("./out/one.md"), "{shown}");
    assert!(!shown.contains("writing no file"), "{shown}");
}

/// A run refused before its first frame settles as refused, with its reason (live gauntlet 4,
/// r09 B15: it stayed « run (starting) »): its label, its standing on the pinned row, its
/// settlement row and its stream say so, in either language, and nothing ran. A leg whose run
/// already started keeps its own frames' word.
#[test]
fn a_run_refused_before_its_start_settles_as_refused_with_its_reason() {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    let why = "NIKA-1709 · unavoidable cost floor $0.0021 over the ceiling $0.0010";
    run.refuse(why.to_owned());
    run.ended();
    assert_eq!(run.label(), "run (refused)");
    let pinned = run.pinned("demo");
    assert_eq!(pinned.state, TaskState::Failed);
    assert_eq!(pinned.words, format!("refused · {why}"));
    let (_, body) = run.view(
        RunFace::Run,
        &Pick::new(),
        200,
        (Voice::default(), true),
        false,
    );
    let shown = body
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for said in [
        format!("two.nika · a fresh run · refused · {why}"),
        format!("refused before its start · {why} · nothing ran"),
        "stream · its refusal alone, before any frame".to_owned(),
    ] {
        assert!(shown.contains(&said), "{said}\n{shown}");
    }
    assert!(!shown.contains("no frame received"), "{shown}");
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    assert_eq!(
        run.pinned_in("demo", french).words,
        format!("refusé · {why}")
    );
    let mut started = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    started.apply(start(EXEC, SOURCE));
    started.refuse(why.to_owned());
    assert_ne!(started.label(), "run (refused)");
    assert_eq!(started.pinned("demo").words, "running");
}

/// Declared `zeta` then `alpha` (which reads it): the frames schedule them alphabetically,
/// the run executes zeta first, and its rows in order follow the graph the run names.
const ORDER: &str = "nika: order\npermits: {}\ntasks:\n  zeta:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n  alpha:\n    with: { x: \"${{ tasks.zeta.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: two } }\n";

/// The rows in the order the run executes them (live gauntlet 4, r06 B12): bound to the bytes
/// it names, the graph's waves, never the frames' alphabetical scheduling.
#[test]
fn the_rows_follow_the_order_the_run_executes() {
    let look = crate::session::judge_for_tests("order.nika", "w".repeat(64), ORDER);
    let mut run = LiveRun::asked("order.nika".to_owned(), false, true, Some(look));
    run.apply(start(EXEC, ORDER));
    run.apply(task(EXEC, 2, "task_scheduled", "alpha"));
    run.apply(task(EXEC, 3, "task_scheduled", "zeta"));
    run.apply(task(EXEC, 4, "task_completed", "zeta"));
    run.apply(task(EXEC, 5, "task_completed", "alpha"));
    assert_eq!(run.binding(), Binding::Same);
    let first_seen: Vec<&str> = run
        .run_view()
        .rows()
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(first_seen, ["alpha", "zeta"], "the frames' own order");
    let ordered: Vec<&str> = (run.rows_in_order().into_iter())
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ordered, ["zeta", "alpha"]);
}

/// The task detail and the outputs, files and proof faces say their own words in the person's
/// language (live gauntlet 4: the run's cards and faces stayed English beside a French
/// conversation): the detail's head, standing, measures and output, what each face lacks,
/// what it read and the verifier's verdict. The task's id, the paths, the trace, the
/// verifier's lines and the reasons a host gave keep theirs; English keeps English.
#[test]
fn the_task_detail_and_the_run_faces_speak_french() {
    use nika_tui_view::Canvas;
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    let shown = |run: &LiveRun, face: RunFace, voice: Voice| {
        let (title, body) = run.view(face, &Pick::new(), 100, (voice, true), false);
        std::iter::once(title)
            .chain(body)
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_scheduled", "save"));
    for frame in wrote(EXEC, 3, "save", "./out/copy.md") {
        run.apply(frame);
    }
    let outputs = shown(&run, RunFace::Outputs, french);
    assert!(
        outputs.contains("pas encore de clôture : les sorties sont inconnues, jamais vides"),
        "{outputs}"
    );
    let proof = shown(&run, RunFace::Proof, french);
    assert!(
        proof.contains("pas encore de clôture : il n'y a aucun journal à vérifier"),
        "{proof}"
    );
    run.apply(settled(EXEC, "succeeded"));
    let files = shown(&run, RunFace::Files, french);
    for said in ["./out/copy.md · annoncé écrit par `save`", "pas encore lu"] {
        assert!(files.contains(said), "{said}\n{files}");
    }
    run.fetched(crate::session::acquire::Fetched::refused(
        "./out/copy.md",
        "a symlink",
    ));
    let files = shown(&run, RunFace::Files, french);
    assert!(files.contains("non lu · a symlink"), "{files}");
    let doc = serde_json::json!({"tier": "ok", "exit": 0, "chain": {"events": 5,
        "head": "cd".repeat(32), "headline": "intact"}, "lines": ["UNSEALED"]});
    run.proven(Proven::judged(".nika/traces/t.ndjson", doc, Vec::new()));
    let proof = shown(&run, RunFace::Proof, french);
    for said in [
        "journal .nika/traces/t.ndjson · tel que la clôture l'a nommé",
        "verdict · OK · sortie 0 · celui du vérificateur, sur ces octets",
        "le journal de ce run · son exécution, sa source et son reçu concordent",
        "chaîne intact · 5 événements · tête cdcdcdcdcdcd",
        "UNSEALED",
        "un journal vérifié enregistre ce qui s'est passé",
    ] {
        assert!(proof.contains(said), "{said}\n{proof}");
    }
    let (title, detail) = run.detail("save", Canvas::new(100, false, false), french);
    let detail = std::iter::once(title)
        .chain(detail)
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    for said in [
        "tâche save · Retour arrière : retour aux tâches",
        "save · réussie · telle que le flux l'a repliée",
        "mesuré · ",
        "sortie · 15 octets sur le flux",
    ] {
        assert!(detail.contains(said), "{said}\n{detail}");
    }
    // The terminal frame's own state (« succeeded ») is the journal's value, kept.
    for english in [
        "reported written by",
        "not read yet",
        "this run's journal",
        "Backspace: back",
        "as the stream folded it",
        "measured",
    ] {
        let all = format!("{files}\n{proof}\n{detail}");
        assert!(!all.contains(english), "{english}\n{all}");
    }
    let english = shown(&run, RunFace::Files, Voice::default());
    assert!(english.contains("not read · a symlink"), "{english}");
}

/// A leg whose gate `first` the person decided `decision`, `second` skipped, settled succeeded
/// as the runtime settles it, bound to the bytes in view.
fn gated(decision: &str) -> LiveRun {
    let mut run = LiveRun::asked("two.nika".to_owned(), false, true, Some(look()));
    run.apply(start(EXEC, SOURCE));
    run.apply(task(EXEC, 2, "task_started", "first"));
    let decided = format!(
        r#"{{"key":"task","value":"first"}},{{"key":"decision","value":"{decision}"}},{{"key":"source","value":"resume"}}"#
    );
    run.apply(event(EXEC, 3, "approval_decided", &decided));
    run.apply(task(EXEC, 4, "task_completed", "first"));
    run.apply(task(EXEC, 5, "task_skipped", "second"));
    run.apply(settled(EXEC, "succeeded"));
    run
}

/// A gate the person refused reads as where the run stopped in every view of the run (live
/// gauntlet 5, r04 B9: « ✔ » and « succeeded » after a « non »): the run's standing, the
/// graph's chip, the task's row and its detail say their « no » in the stop glyph, though the
/// settlement says succeeded; a gate they allowed reads done.
#[test]
fn a_refused_gate_reads_as_the_runs_stop_in_every_view_of_the_run() {
    let run = gated("deny");
    let shown = text(&run);
    for said in [
        "settled · stopped at your no",
        "⊘ answered no",
        "⊘ first · 0 ms · answered no",
    ] {
        assert!(shown.contains(said), "{said}\n{shown}");
    }
    assert!(
        !shown.contains("succeeded") && !shown.contains("✔"),
        "{shown}"
    );
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    let (_, detail) = run.detail(
        "first",
        nika_tui_view::Canvas::new(100, false, false),
        french,
    );
    let detail = (detail.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        detail.contains("⊘ first · répondu non : le run s'est arrêté là"),
        "{detail}"
    );
    assert!(!detail.contains("réussi"), "{detail}");
    let (_, ascii) = run.lines(RunFace::Run, 100, true, false);
    let ascii = (ascii.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for said in ["x answered no", "x first - 0 ms - answered no"] {
        assert!(ascii.contains(said), "{said}\n{ascii}");
    }
    let allowed = text(&gated("allow"));
    assert!(allowed.contains("settled · succeeded"), "{allowed}");
    assert!(allowed.contains("✔ first · 0 ms"), "{allowed}");
    assert!(!allowed.contains("answered no"), "{allowed}");
}
