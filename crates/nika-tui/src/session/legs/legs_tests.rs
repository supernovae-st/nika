// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The host's own record of a leg: bound at its first frame, its one start's
//! hash, the paths its writes reported, the receipt its settlement named;
//! nothing from another execution or after the settlement; a kept identity
//! converted back to the very execution.

use super::*;

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const OTHER: &str = "01a0ef11-0212-70de-a8b3-99de94270000";

fn event(exec: &str, n: u32, kind: &str, fields: &str) -> RunFrame {
    let line = format!(
        r#"{{"correlation":null,"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
    );
    RunFrame::decode(&line).expect("a runtime event")
}

fn start(exec: &str, n: u32, hash: &str) -> RunFrame {
    event(
        exec,
        n,
        "workflow_started",
        &format!(r#"{{"key":"workflow_sha256","value":"{hash}"}}"#),
    )
}

fn write(exec: &str, n: u32, task: &str, path: &str) -> [RunFrame; 2] {
    let started = format!(
        r#"{{"key":"task","value":"{task}"}},{{"key":"note","value":"invoke · nika:write"}}"#
    );
    let output = serde_json::to_string(&serde_json::to_string(path).expect("json")).expect("json");
    let completed =
        format!(r#"{{"key":"task","value":"{task}"}},{{"key":"output","value":{output}}}"#);
    [
        event(exec, n, "task_started", &started),
        event(exec, n + 1, "task_completed", &completed),
    ]
}

fn settled(exec: &str) -> RunFrame {
    let line = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{{"uuid":"{exec}"}},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed","receipt":{{"trace_path":".nika/traces/t.ndjson","chain_head":"cd","chain_len":7}}}}"#
    );
    RunFrame::decode(&line).expect("a settlement")
}

#[test]
fn a_leg_keeps_what_the_host_relayed_and_nothing_else() {
    let mut legs = Legs::default();
    legs.asked();
    legs.frame(&start(EXEC, 1, "aa"));
    legs.frame(&start(OTHER, 2, "bb"));
    for frame in write(EXEC, 3, "save", "./out/copy.md") {
        legs.frame(&frame);
    }
    for frame in write(OTHER, 5, "elsewhere", "./out/other.md") {
        legs.frame(&frame);
    }
    legs.frame(&settled(EXEC));
    for frame in write(EXEC, 7, "late", "./out/late.md") {
        legs.frame(&frame);
    }
    let leg = legs.newest().expect("the leg");
    assert_eq!(leg.written, ["./out/copy.md"]);
    assert_eq!(leg.workflow_sha256(), Some("aa"));
    let expect = leg.proof_expectation();
    assert_eq!(expect.execution, leg.execution);
    assert_eq!(
        (expect.chain_head.as_deref(), expect.chain_len),
        (Some("cd"), Some(7))
    );
    assert_eq!(leg.trace.as_deref(), Some(".nika/traces/t.ndjson"));
}

#[test]
fn two_starts_name_no_source_hash() {
    let mut legs = Legs::default();
    legs.asked();
    legs.frame(&start(EXEC, 1, "aa"));
    legs.frame(&start(EXEC, 2, "aa"));
    assert_eq!(legs.newest().expect("the leg").workflow_sha256(), None);
}

#[test]
fn a_kept_identity_converts_back_to_the_very_execution() {
    let mut legs = Legs::default();
    legs.asked();
    legs.frame(&start(EXEC, 1, "aa"));
    legs.frame(&settled(EXEC));
    let leg = legs.newest().expect("the leg").clone();
    let kept = leg
        .kept_run()
        .ended(Some(std::path::Path::new("two.nika")), 0, None);
    assert_eq!(kept.execution.as_deref(), Some(EXEC));
    let mut later = Legs::default();
    let execution = later.kept(&kept).expect("an execution");
    assert_eq!(execution, leg.execution);
    let restored = later.find(&execution).expect("kept");
    assert!(
        restored.kept && restored.written.is_empty(),
        "a record lists no file"
    );
    assert_eq!(restored.proof_expectation(), leg.proof_expectation());
    assert!(
        later.newest().is_none(),
        "a kept leg is never the run just relayed"
    );
    let mut malformed = KeptRun::new();
    malformed.execution = Some("exe-not-a-uuid".to_owned());
    assert!(Legs::default().kept(&malformed).is_none());
}

/// The Live host reads only what a leg it relayed names: the file the run's
/// write reported (read now), never another path a renderer could pass, and
/// never a leg of an execution it did not relay; the observation then
/// carries the leg's identity into the Session's kept run.
#[test]
fn the_live_host_reads_only_what_a_relayed_leg_names() {
    use crate::model::Conversation;
    use crate::session::feed::{Gap, Seen};
    use crate::session::{Live, Runners};
    use nika_session::intelligence::{
        IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
    };
    use std::sync::Arc;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let room = std::env::temp_dir().join(format!("nika-tui-legs-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(room.join("out")).expect("room");
    let source = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n";
    std::fs::write(room.join("two.nika"), source).expect("workflow");
    std::fs::write(room.join("out/copy.md"), "# Brief\n").expect("written");
    std::fs::write(room.join("out/other.md"), "secret\n").expect("other");
    let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let mut live = Live::new(
        room.clone(),
        IntelligenceCensus::empty(),
        Some(none),
        None,
        Box::new(|_| Box::new(nika_session::ScriptedReasoner::new(Vec::new()))),
        Runners {
            run_once: Box::new(|_, _| panic!("no plain run")),
            run_resume: Box::new(|_, _, _, _| panic!("no resume")),
            run_tapped: None,
        },
    )
    .with_run_tapped_observed(Box::new(|_, _, sink| {
        let frames = [start(EXEC, 1, "aa")]
            .into_iter()
            .chain(write(EXEC, 2, "save", "./out/copy.md"))
            .chain([settled(EXEC)]);
        for frame in frames {
            sink.frame(frame);
        }
        (
            0,
            Some(std::path::PathBuf::from(".nika/traces/t.ndjson")),
            Vec::new(),
        )
    }));
    let _ = live.open();
    let (busy, _said) = std::sync::mpsc::channel();
    let (tx, _rx) = std::sync::mpsc::sync_channel(64);
    let _ = live.submit_observed(
        "run two.nika",
        &busy,
        &Seen::new(tx, Arc::new(Gap::default())),
    );
    let exec = id(EXEC);
    let read = live.fetch(&exec, "./out/copy.md").expect("a host");
    assert_eq!(read.bytes(), Some(&b"# Brief\n"[..]), "{read:?}");
    let other = live.fetch(&exec, "./out/other.md").expect("a host");
    assert!(other.bytes().is_none(), "{other:?}");
    assert!(
        other
            .why()
            .is_some_and(|w| w.contains("not a file this run reported writing"))
    );
    let stranger = live.fetch(&id(OTHER), "./out/copy.md").expect("a host");
    assert!(stranger.bytes().is_none(), "{stranger:?}");
    let unrelayed = live.prove(&id(OTHER)).expect("a host");
    assert!(
        unrelayed
            .why()
            .is_some_and(|w| w.contains("not observed here")),
        "{unrelayed:?}"
    );
    let _ = std::fs::remove_dir_all(&room);
}

fn id(uuid: &str) -> ExecutionId {
    serde_json::from_value(serde_json::json!({ "uuid": uuid })).expect("an execution")
}

/// A resume is a new leg: its own execution, its own journal and receipt (as
/// a real keyless gated run shows); the paused leg keeps its own. Nothing
/// links them beyond the identity each settlement names.
#[test]
fn a_resumed_leg_is_its_own_execution_and_journal() {
    let paused = |exec: &str, trace: &str| {
        let line = format!(
            r#"{{"kind":"run_settled","status":"paused","cause":"normal","execution":{{"uuid":"{exec}"}},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed","receipt":{{"trace_path":"{trace}","chain_head":"ab","chain_len":4}}}}"#
        );
        RunFrame::decode(&line).expect("a settlement")
    };
    let mut legs = Legs::default();
    legs.asked();
    legs.frame(&start(EXEC, 1, "aa"));
    legs.frame(&paused(EXEC, ".nika/traces/first.ndjson"));
    legs.asked();
    legs.frame(&start(OTHER, 2, "aa"));
    legs.frame(&settled(OTHER));
    let first = legs.find(&id(EXEC)).expect("the paused leg");
    assert_eq!(first.trace.as_deref(), Some(".nika/traces/first.ndjson"));
    let resumed = legs.newest().expect("the resumed leg");
    assert_eq!(resumed.execution, id(OTHER));
    assert_eq!(resumed.trace.as_deref(), Some(".nika/traces/t.ndjson"));
}

/// A reasoner that counts every call it is asked to make.
struct Counting(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl nika_session::SessionReasoner for Counting {
    fn name(&self) -> String {
        "counting".to_owned()
    }
    fn reason(&mut self, _prompt: &str) -> Result<nika_session::Reply, nika_session::ReasonError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(nika_session::ReasonError::Provider(
            "no call is expected".to_owned(),
        ))
    }
}

/// Closed and reopened on the same HOME: the kept turns and the last run are
/// repainted as history; the reopen calls no runner (each panics) and no
/// reasoner, and the kept run's proof only reads its journal.
#[test]
fn a_reopen_repaints_history_and_runs_nothing() {
    use crate::model::{Beat, Conversation};
    use crate::session::feed::{Gap, Seen};
    use crate::session::{Live, Runners};
    use nika_session::intelligence::{
        IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let base = std::env::temp_dir().join(format!("nika-tui-reopen-{}-{nanos}", std::process::id()));
    let (room, home) = (base.join("room"), base.join("home"));
    std::fs::create_dir_all(&room).expect("room");
    std::fs::create_dir_all(&home).expect("home");
    let source = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n";
    std::fs::write(room.join("two.nika"), source).expect("workflow");
    let calls = Arc::new(AtomicUsize::new(0));
    let open = |runners: Runners| {
        let counted = Arc::clone(&calls);
        Live::new(
            room.clone(),
            IntelligenceCensus::empty(),
            Some(UserIntelligencePreference::new(
                IntelligenceKind::None,
                None,
            )),
            Some(home.clone()),
            Box::new(move |_| Box::new(Counting(Arc::clone(&counted)))),
            runners,
        )
    };
    let refused = || Runners {
        run_once: Box::new(|_, _| panic!("no plain run")),
        run_resume: Box::new(|_, _, _, _| panic!("no resume")),
        run_tapped: Some(Box::new(|_, _, _| panic!("no story tap"))),
    };
    let mut first = open(refused()).with_run_tapped_observed(Box::new(|_, _, sink| {
        for frame in [start(EXEC, 1, "aa"), settled(EXEC)] {
            sink.frame(frame);
        }
        (
            0,
            Some(std::path::PathBuf::from(".nika/traces/t.ndjson")),
            Vec::new(),
        )
    }));
    let _ = first.open();
    let (busy, _said) = std::sync::mpsc::channel();
    let (tx, _rx) = std::sync::mpsc::sync_channel(64);
    let _ = first.submit_observed(
        "run two.nika",
        &busy,
        &Seen::new(tx, Arc::new(Gap::default())),
    );
    drop(first);
    let mut again = open(refused())
        .with_run_tapped_observed(Box::new(|_, _, _| panic!("a reopen runs nothing")))
        .with_run_review_observed(Box::new(|_, _, _| panic!("a reopen reviews nothing")));
    let shown: Vec<String> = (again.open().into_iter())
        .filter_map(|beat| match beat {
            Beat::Say(said) => Some(said.text),
            _ => None,
        })
        .collect();
    let shown = shown.join("\n");
    assert!(shown.contains("earlier in this conversation"), "{shown}");
    assert!(
        shown.contains("last run, observed in an earlier session"),
        "{shown}"
    );
    let kept = again.kept_run().expect("kept").expect("readable");
    assert_eq!(kept.execution.as_deref(), Some(EXEC));
    let proven = again.prove(&id(EXEC)).expect("a host");
    assert!(
        proven.why().is_some_and(|w| w.contains("no journal")),
        "{proven:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no reasoner call");
    let _ = std::fs::remove_dir_all(&base);
}

/// A settle frame of `task` in `exec` naming the child journal `trace`.
fn called(exec: &str, n: u32, kind: &str, task: &str, trace: &str) -> RunFrame {
    let row = serde_json::json!({"target": "./child.nika", "trace_id": trace,
        "chain_head": "ab", "def_hash": "cd", "outcome": "success"})
    .to_string();
    let fields = format!(
        r#"{{"key":"task","value":"{task}"}},{{"key":"child","value":{}}}"#,
        serde_json::to_string(&row).expect("json")
    );
    event(exec, n, kind, &fields)
}

/// The host learns a child relation from the same admissible settle the
/// fold reads, for the leg it binds only: never from a start, another
/// execution or a frame after the settlement; a new attempt drops it.
#[test]
fn the_host_keeps_the_child_relation_of_an_admissible_settle_only() {
    let mut legs = Legs::default();
    legs.asked();
    legs.frame(&start(EXEC, 1, "aa"));
    legs.frame(&called(EXEC, 2, "task_started", "call", "early.ndjson"));
    assert!(legs.find(&id(EXEC)).and_then(|l| l.child("call")).is_none());
    let settle = called(EXEC, 3, "task_completed", "call", "child.ndjson");
    legs.frame(&settle);
    legs.frame(&called(
        OTHER,
        4,
        "task_completed",
        "other",
        "foreign.ndjson",
    ));
    let leg = legs.find(&id(EXEC)).expect("the leg");
    let RunFrame::Event(settled_event) = &settle else {
        panic!("an event");
    };
    assert_eq!(
        leg.child("call"),
        nika_display::run_story::ChildRun::of(settled_event).as_ref()
    );
    assert!(leg.child("other").is_none());
    legs.frame(&event(
        EXEC,
        5,
        "task_started",
        r#"{"key":"task","value":"call"}"#,
    ));
    assert!(
        legs.find(&id(EXEC)).and_then(|l| l.child("call")).is_none(),
        "a new attempt drops it"
    );
    legs.frame(&called(EXEC, 6, "task_completed", "call", "second.ndjson"));
    legs.frame(&settled(EXEC));
    legs.frame(&called(EXEC, 7, "task_completed", "call", "late.ndjson"));
    let kept = (legs.find(&id(EXEC)).and_then(|l| l.child("call"))).expect("kept");
    assert_eq!(kept.trace_id.as_deref(), Some("second.ndjson"));
}
