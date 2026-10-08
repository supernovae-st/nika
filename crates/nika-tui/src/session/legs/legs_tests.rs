// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The host's own record of a leg: bound at its first frame, its one start's
//! hash, the paths its writes reported, the receipt its settlement named;
//! nothing from another execution or after the settlement; a kept identity
//! converted back to the very execution.

use super::*;

pub(crate) const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
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
    assert_eq!(leg.trace().as_deref(), Some(".nika/traces/t.ndjson"));
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
    assert_eq!(first.trace().as_deref(), Some(".nika/traces/first.ndjson"));
    let resumed = legs.newest().expect("the resumed leg");
    assert_eq!(resumed.execution, id(OTHER));
    assert_eq!(resumed.trace().as_deref(), Some(".nika/traces/t.ndjson"));
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

/// The sha256 of `bytes`, hex.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{b:02x}"));
            hex
        })
}

/// A chained journal of the run `EXEC` over the source hash `aa` whose task
/// `save` wrote `./out/copy.md` and whose close carries the fields `close`,
/// and its head and length.
pub(crate) fn written_journal(close: serde_json::Value) -> (String, String, u64) {
    let frames: [(&str, serde_json::Value); 4] = [
        (
            "workflow_started",
            serde_json::json!([{"key": "workflow", "value": "two"},
            {"key": "workflow_sha256", "value": "aa"}]),
        ),
        (
            "task_started",
            serde_json::json!([{"key": "task", "value": "save"},
            {"key": "note", "value": "invoke · nika:write"}]),
        ),
        (
            "task_completed",
            serde_json::json!([{"key": "task", "value": "save"},
            {"key": "output", "value": "\"./out/copy.md\""}]),
        ),
        ("workflow_completed", close),
    ];
    let mut chain = sha256_hex(b"nika-trace-v1");
    let mut out = String::new();
    for (n, (kind, fields)) in frames.into_iter().enumerate() {
        let line = serde_json::json!({
            "chain": chain, "correlation": null, "execution": {"uuid": EXEC}, "fields": fields,
            "id": {"uuid": format!("01a0ef11-03a1-73d9-a2bc-{n:012x}")},
            "kind": kind, "run": null, "timestamp": n,
        })
        .to_string();
        chain = sha256_hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
    }
    (out, chain, 4)
}

/// A settlement of `exec` whose receipt names the journal's head and length.
pub(crate) fn settled_at(exec: &str, head: &str, len: u64) -> RunFrame {
    let line = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{{"uuid":"{exec}"}},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed","receipt":{{"trace_path":".nika/traces/t.ndjson","chain_head":"{head}","chain_len":{len}}}}}"#
    );
    RunFrame::decode(&line).expect("a settlement")
}

/// A host reopened on a HOME whose last run (EXEC) left `written_journal`.
fn reopened(base: &std::path::Path) -> crate::session::Live {
    let close = serde_json::json!([{"key": "status", "value": "succeeded"}]);
    reopened_closing(base, close, &std::sync::Arc::default())
}

/// A host reopened on a HOME whose last run (`EXEC`) left
/// `written_journal(close)`, its reasoner counting each call into `calls`;
/// every runner refuses: nothing may run.
pub(crate) fn reopened_closing(
    base: &std::path::Path,
    close: serde_json::Value,
    calls: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> crate::session::Live {
    use crate::model::Conversation;
    use crate::session::feed::{Gap, Seen};
    use crate::session::{Live, Runners};
    use nika_session::intelligence::{
        IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
    };
    use std::sync::Arc;

    let (room, home) = (base.join("room"), base.join("home"));
    std::fs::create_dir_all(room.join(".nika/traces")).expect("room");
    std::fs::create_dir_all(room.join("out")).expect("out");
    std::fs::create_dir_all(&home).expect("home");
    let source = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n";
    std::fs::write(room.join("two.nika"), source).expect("workflow");
    let (raw, head, len) = written_journal(close);
    std::fs::write(room.join(".nika/traces/t.ndjson"), raw).expect("journal");
    std::fs::write(room.join("out/copy.md"), "# Copy\n").expect("written");
    let open = |runners: Runners| {
        let counted = Arc::clone(calls);
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
    let mut first = open(refused()).with_run_tapped_observed(Box::new(move |_, _, sink| {
        for frame in [start(EXEC, 1, "aa"), settled_at(EXEC, &head, len)] {
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
    let _ = again.open();
    assert!(again.kept_run().is_some_and(|k| k.is_ok()), "a kept run");
    again
}

/// A kept run's verified journal lends the host the names its writes
/// reported only when the shell adopts that very reading: before, after a
/// refresh that captured again and was refused, or for another run, the
/// file stays unreadable, and the leg's own commitments never move.
#[test]
fn a_kept_journal_lends_its_written_names_only_on_a_current_adoption() {
    use crate::model::Conversation;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let base = std::env::temp_dir().join(format!("nika-tui-adopt-{}-{nanos}", std::process::id()));
    let mut host = reopened(&base);
    let expectation = |host: &crate::session::Live| {
        host.legs
            .lock()
            .ok()
            .and_then(|legs| legs.find(&id(EXEC)).map(Leg::proof_expectation))
    };
    let before = expectation(&host);
    let proven = host.prove(&id(EXEC)).expect("a host");
    assert!(
        proven.why().is_none() && proven.unbound().is_empty(),
        "{proven:?}"
    );
    let unread = host.fetch(&id(EXEC), "./out/copy.md").expect("a host");
    assert!(
        unread.bytes().is_none(),
        "nothing is lent before the adoption: {unread:?}"
    );
    assert!(
        !host.adopt(&id(OTHER), &proven),
        "another run adopts nothing"
    );
    assert!(
        host.adopt(&id(EXEC), &proven),
        "the current reading is adopted"
    );
    let read = host.fetch(&id(EXEC), "./out/copy.md").expect("a host");
    assert_eq!(read.bytes(), Some(&b"# Copy\n"[..]), "{read:?}");
    assert_eq!(
        expectation(&host),
        before,
        "the kept commitments never move"
    );
    // A refresh captures again; the journal is now corrupted: refused, and
    // the earlier lending is gone with it (a late adoption of the earlier
    // reading restores nothing either).
    let journal = base.join("room/.nika/traces/t.ndjson");
    let mut raw = std::fs::read_to_string(&journal).expect("journal");
    raw.insert(10, 'x');
    std::fs::write(&journal, raw).expect("corrupt");
    let kept = host.fetch(&id(EXEC), "./out/copy.md").expect("a host");
    assert!(
        kept.bytes().is_some(),
        "changed after its capture, lent the same: {kept:?}"
    );
    let refused = host.prove(&id(EXEC)).expect("a host");
    assert!(refused.events().is_none(), "{refused:?}");
    let gone = host.fetch(&id(EXEC), "./out/copy.md").expect("a host");
    assert!(
        gone.bytes().is_none(),
        "revoked by the new capture: {gone:?}"
    );
    assert!(
        !host.adopt(&id(EXEC), &refused),
        "a refused reading lends nothing"
    );
    assert!(
        !host.adopt(&id(EXEC), &proven),
        "nor the earlier reading, late"
    );
    assert!(
        host.fetch(&id(EXEC), "./out/copy.md")
            .expect("a host")
            .bytes()
            .is_none()
    );
    assert_eq!(expectation(&host), before);
    let _ = std::fs::remove_dir_all(&base);
}

/// A kept leg adopts the child relations its journal names, by the live rule
/// (a new attempt drops one), from the reading captured last only; a new
/// capture forgets them, and its commitments never move.
#[test]
fn a_kept_leg_adopts_child_relations_from_its_last_capture_only() {
    let mut run = nika_session::KeptRun::new();
    run.execution = Some(EXEC.to_owned());
    run.trace = Some(".nika/traces/t.ndjson".to_owned());
    run.workflow_sha256 = Some("aa".to_owned());
    (run.chain_head, run.chain_len) = (Some("cd".repeat(32)), Some(4));
    let mut legs = Legs::default();
    let execution = legs.kept(&run).expect("a kept leg");
    let frames = [
        start(EXEC, 1, "aa"),
        called(EXEC, 2, "task_completed", "call", "first.ndjson"),
        called(EXEC, 3, "task_completed", "again", "second.ndjson"),
        event(EXEC, 4, "task_started", r#"{"key":"task","value":"again"}"#),
    ];
    let events = (frames.into_iter())
        .filter_map(|f| match f {
            RunFrame::Event(event) => Some(*event),
            _ => None,
        })
        .collect();
    let doc = serde_json::json!({"exit": 0});
    let proven = Proven::judged(".nika/traces/t.ndjson", doc, Vec::new()).lending(events);
    let leg = legs.find_mut(&execution).expect("kept");
    let commitments = leg.proof_expectation();
    assert!(!leg.adopt(&proven), "not the reading captured last");
    leg.captured(&proven);
    assert!(leg.adopt(&proven));
    let trace = |leg: &Leg, task: &str| leg.child(task).and_then(|c| c.trace_id.clone());
    assert_eq!(trace(leg, "call").as_deref(), Some("first.ndjson"));
    assert_eq!(trace(leg, "again"), None, "a new attempt drops it");
    assert_eq!(leg.proof_expectation(), commitments);
    leg.forget_history();
    assert_eq!(trace(leg, "call"), None, "a new capture forgets it");
    assert!(
        !leg.adopt(&proven),
        "the earlier reading is no longer the last"
    );
    assert_eq!(leg.proof_expectation(), commitments);
}

/// The last capture lends only what its own projection admitted: a capture
/// of the same bytes whose projection was refused (the same witness, no
/// events) leaves nothing adoptable, so a Proof read before it adopts
/// nothing late; files and child relations stay unlent and the commitments
/// never move. An admitted capture is still adopted.
#[test]
fn a_refused_capture_of_the_same_bytes_leaves_nothing_adoptable() {
    let mut run = nika_session::KeptRun::new();
    run.execution = Some(EXEC.to_owned());
    run.trace = Some(".nika/traces/t.ndjson".to_owned());
    run.workflow_sha256 = Some("aa".to_owned());
    (run.chain_head, run.chain_len) = (Some("cd".repeat(32)), Some(4));
    let mut legs = Legs::default();
    let execution = legs.kept(&run).expect("a kept leg");
    let mut frames = vec![start(EXEC, 1, "aa")];
    frames.extend(write(EXEC, 2, "save", "./out/copy.md"));
    frames.push(called(EXEC, 4, "task_completed", "call", "child.ndjson"));
    let events = (frames.into_iter())
        .filter_map(|f| match f {
            RunFrame::Event(event) => Some(*event),
            _ => None,
        })
        .collect();
    let trace = ".nika/traces/t.ndjson";
    let lent = Proven::judged(trace, serde_json::json!({"exit": 0}), Vec::new()).lending(events);
    let refused = Proven::judged(trace, serde_json::json!({"exit": 1}), Vec::new());
    assert_eq!(
        lent.witness(),
        refused.witness(),
        "the same bytes, captured twice"
    );
    assert!(lent.events().is_some() && refused.events().is_none());
    let leg = legs.find_mut(&execution).expect("kept");
    let commitments = leg.proof_expectation();
    leg.captured(&lent);
    assert!(leg.adopt(&lent), "an admitted capture is adopted");
    assert_eq!(leg.written, ["./out/copy.md"]);
    assert!(leg.child("call").is_some());
    leg.forget_history();
    leg.captured(&refused);
    assert!(
        !leg.adopt(&lent),
        "the last capture lent nothing: the earlier Proof adopts nothing"
    );
    assert!(
        leg.written.is_empty() && leg.child("call").is_none(),
        "nothing lent"
    );
    assert_eq!(leg.proof_expectation(), commitments);
}

/// The variable naming the directory the custody child runs in: set only by
/// its parent below, so the child never touches a real HOME.
const CUSTODY_CHILD: &str = "NIKA_TUI_F2_CUSTODY_CHILD";

/// The F2 witness in a real verifier context: the same journal bytes are
/// captured twice by a real host and the only change between the two is the
/// custody key file the verifier reads before judging (an oversized
/// `run-signing.pub`: a custody-read refusal, not a cryptographic
/// revocation). It runs as a child process of this test binary whose HOME
/// is a fresh directory, set on that child alone.
#[test]
#[allow(
    clippy::disallowed_types,
    reason = "the witness spawns this very test binary, its HOME set on the child alone"
)]
fn a_custody_refusal_over_the_same_bytes_leaves_nothing_adoptable() {
    use std::io::Write as _;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let name = format!("nika-tui-custody-{}-{nanos}", std::process::id());
    let base = std::env::temp_dir().join(name);
    std::fs::create_dir_all(base.join("home")).expect("home");
    let child = "session::legs::legs_tests::custody_child";
    let out = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", child, "--test-threads=1"])
        .env(CUSTODY_CHILD, &base)
        .env("HOME", base.join("home"))
        .env_remove("USERPROFILE")
        .output()
        .expect("the child runs");
    let context = std::fs::read_to_string(base.join("custody-context.txt")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&base);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(&format!("{child} ... ok")),
        "the child ran: {said}"
    );
    assert!(context.contains("late adoption of A: false"), "{context}");
    let _ = writeln!(std::io::stdout(), "custody child context:\n{context}");
}

/// The custody child: nothing unless its parent named its directory.
#[test]
fn custody_child() {
    use crate::model::Conversation;
    #[allow(clippy::disallowed_methods, reason = "set only by the parent test")]
    let Some(base) = std::env::var_os(CUSTODY_CHILD).map(std::path::PathBuf::from) else {
        return;
    };
    let close = serde_json::json!([{"key": "status", "value": "succeeded"}]);
    let mut host = reopened_closing(&base, close, &std::sync::Arc::default());
    let first = host.prove(&id(EXEC)).expect("a host");
    assert!(first.events().is_some(), "A is admitted: {first:?}");
    assert!(host.adopt(&id(EXEC), &first), "A is adopted");
    let lent = host.fetch(&id(EXEC), "./out/copy.md").expect("a host");
    assert!(lent.bytes().is_some(), "{lent:?}");
    let keys = base.join("home/.nika/keys");
    std::fs::create_dir_all(&keys).expect("keys");
    std::fs::write(keys.join("run-signing.pub"), vec![b'x'; 64 * 1024 + 1]).expect("custody");
    let second = host.prove(&id(EXEC)).expect("a host");
    assert_eq!(second.witness(), first.witness(), "the very same bytes");
    assert!(second.events().is_none(), "B is refused: {second:?}");
    let late = host.adopt(&id(EXEC), &first);
    let gone = host.fetch(&id(EXEC), "./out/copy.md").expect("a host");
    let context = format!(
        "HOME {}\nwitness A {:?} = B {:?}\nA tier {:?} terminal {:?} events {}\nB tier {:?} why {:?} projection {:?}\nlate adoption of A: {late}\nfile after: {:?}\n",
        base.join("home").display(),
        first.witness(),
        second.witness(),
        first.tier(),
        first.terminal(),
        first.events().map_or(0, <[_]>::len),
        second.tier(),
        second.why(),
        second.projection_why(),
        gone.bytes().map(<[u8]>::len),
    );
    std::fs::write(base.join("custody-context.txt"), &context).expect("context");
    assert!(
        !late,
        "the earlier Proof adopts nothing after a refused capture: {context}"
    );
    assert!(gone.bytes().is_none(), "{context}");
}
