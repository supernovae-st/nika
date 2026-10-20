// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The offer about a result, from the person's keys to what the Session keeps. An opinion is
//! earned the native way, never injected: a scripted author on the loopback local route (a double
//! of what an intelligence answers, never of what Nika decides) writes and proposes a candidate
//! that only writes one file; the person saves and runs it in words; the run is observed through
//! the conversation's own `perform`; the person picks an adjustment by its identity; the author
//! proposes the revision; it is saved and run; only then the opinion waits. There the composer
//! takes the person's keys, `Said` leaves by its door to the real `Live`, and the Session keeps
//! what it keeps, as a reopen reads it back. The environment is the child's own (a test cannot
//! set one in-process): each case re-runs this binary as a child against its own peer.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_session::reasoner::{NoReasoner, ProviderReasoner};
use nika_session::work::{Feedback, FeedbackKind, FeedbackOffer};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, RunRequest, SessionReasoner,
    UserIntelligencePreference,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{Said, picked, routed};
use crate::composer::Composer;
use crate::composer::answer::{Offered, Sent};
use crate::model::{Beat, Conversation, Waiting};
use crate::session::feed::{Gap, Seen};
use crate::session::{Live, Runners};

/// The person's local engine route, as the session was opened on it.
const SEAT: &str = "vllm/oux-author";
/// The one file the candidate writes, as the person's request names it.
const NOTE: &str = "./hello.md";
/// The person's request: the file and its words, named.
const REQUEST: &str = "écris bonjour dans ./hello.md";
/// Save and Run, said in words at the proposal.
const SAVE_AND_RUN: &str = "parfait, enregistre-le puis lance-le";
/// The words of `SAVE_AND_RUN` that ask for both acts.
const SAVE_AND_RUN_WORDS: &str = "enregistre-le puis lance-le";
/// What the first run writes, then the amended one.
const FIRST: &str = "bonjour\n";
const SECOND: &str = "# bonjour\n";
/// The words typed at the opinion: its first row's whole label in three cases and spacings,
/// « Later » twice, and a row's key.
const TYPED: [&str; 6] = [
    "Suitable for this use",
    "suitable for this use",
    " SUITABLE FOR THIS USE ",
    "Later",
    "later",
    "suitable",
];
/// The options a verifier question offers when it approves, and when it locates nothing.
const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];
const UNLOCATED: &str = "unlocated";

/// Words typed at a genuine opinion, each on its own fresh offer (the first words close it): the
/// person's own line, never the pick of a row. The offer closes unanswered, the records stay
/// exactly the adjustment it follows, no run or artifact moves, and a reopen keeps the same.
#[test]
fn words_typed_at_a_genuine_opinion_are_the_persons_line_and_record_no_opinion() {
    let failed: Vec<String> = (TYPED.iter())
        .filter_map(|typed| run_child("words", typed).err())
        .collect();
    assert!(
        failed.is_empty(),
        "typed words at a genuine opinion offer:\n{}",
        failed.join("\n")
    );
}

/// A genuine opinion opens on nothing (Enter alone sends nothing), and its first row selected
/// with Down then Enter records exactly that one opinion, bound to the offer's identity and the
/// result it is about; the same act repeated changes nothing, and a reopen keeps it.
#[test]
fn a_row_picked_at_a_genuine_opinion_records_exactly_one_opinion() {
    run_child("row", "").unwrap_or_else(|why| panic!("{why}"));
}

/// The child: earn the opinion its parent's peer scripts, then act the case its parent named.
#[test]
#[ignore = "run by the bridge's parent tests in a child process"]
fn child() {
    let Some(case) = std::env::var_os("BRIDGE_CASE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("BRIDGE_ROOT").expect("the project"));
    let home = PathBuf::from(std::env::var_os("HOME").expect("the home"));
    let typed = (std::env::var_os("BRIDGE_TYPED"))
        .map_or_else(String::new, |typed| typed.to_string_lossy().into_owned());
    match case.to_str() {
        Some("words") => words(&root, &home, &typed),
        Some("row") => row(&root, &home),
        other => panic!("no bridge case {other:?}"),
    }
}

/// `typed` at a genuine opinion: the person's line, nothing kept of it.
fn words(root: &Path, home: &Path, typed: &str) {
    let runs = Arc::new(AtomicUsize::new(0));
    let (busy, seen) = sinks();
    let mut live = live(root, home, &runs);
    let opinion = earn_opinion(&mut live, &busy, &seen);
    let earned = feedback(&live);
    lineage(&earned);
    let written = std::fs::read(root.join(NOTE)).expect("the note");
    let (sent, as_what, said) = enter_after(&opinion, typed, false);
    let said = said.unwrap_or_else(|| panic!("{typed:?}: Enter sent nothing ({sent:?})"));
    let turn = said.send(&mut live, &busy, &seen);
    let after = feedback(&live);
    assert!(
        after.recorded == earned.recorded,
        "{typed:?} left as {as_what:?} {said:?}: the Session kept {:?}",
        after.recorded
    );
    assert!(
        after.offer.is_none(),
        "{typed:?}: the offer closes unanswered"
    );
    assert_eq!(as_what, Some(Sent::Own), "{typed:?}: the person's own line");
    assert_eq!(
        said,
        Said::Answer {
            text: typed.to_owned(),
            witness: witness(&opinion)
        },
        "{typed:?}: the words door, the words as typed"
    );
    assert!(turn.handoff.is_none(), "{typed:?}: nothing handed over");
    assert_eq!(runs.load(Ordering::SeqCst), 2, "{typed:?}: no run");
    let now = std::fs::read(root.join(NOTE)).expect("the note");
    assert_eq!(now, written, "{typed:?}: the artifact unchanged");
    drop(live);
    let reopened = reopen(root, home, &runs);
    assert!(reopened.offer.is_none(), "a reopen offers nothing again");
    assert_eq!(
        reopened.recorded, earned.recorded,
        "{typed:?}: kept exactly the adjustment"
    );
}

/// A genuine opinion answered by its first row, natively.
fn row(root: &Path, home: &Path) {
    let runs = Arc::new(AtomicUsize::new(0));
    let (busy, seen) = sinks();
    let mut live = live(root, home, &runs);
    let opinion = earn_opinion(&mut live, &busy, &seen);
    let earned = feedback(&live);
    let offer = lineage(&earned);
    assert_eq!(
        enter_after(&opinion, "", false),
        (Offered::Pass, None, None),
        "nothing typed, nothing selected: Enter sends nothing"
    );
    assert_eq!(feedback(&live), earned, "no default stands for the person");
    let (sent, as_what, said) = enter_after(&opinion, "", true);
    assert_eq!(
        (sent, as_what),
        (Offered::Answer("suitable".to_owned()), Some(Sent::Offer))
    );
    let said = said.expect("Down then Enter picked the first row");
    let pick = Said::Pick {
        keys: "suitable".to_owned(),
        witness: witness(&opinion),
    };
    assert_eq!(said, pick);
    let turn = said.send(&mut live, &busy, &seen);
    assert!(turn.handoff.is_none(), "an opinion runs nothing");
    let after = feedback(&live);
    assert!(after.offer.is_none(), "the offer was answered");
    assert_eq!(after.recorded.len(), 2, "{:?}", after.recorded);
    assert_eq!(after.recorded[..1], earned.recorded[..]);
    let record = &after.recorded[1];
    assert_eq!(record.kind, FeedbackKind::Opinion);
    assert_eq!(record.picked, ["suitable"]);
    assert_eq!(record.question, offer.question.id.as_str());
    assert_eq!(record.scope, offer.scope);
    assert_eq!(record.follows, offer.follows);
    let again = pick.send(&mut live, &busy, &seen);
    assert!(again.handoff.is_none(), "the stale act hands nothing over");
    assert_eq!(feedback(&live), after, "the stale act records nothing more");
    assert_eq!(runs.load(Ordering::SeqCst), 2, "no run");
    drop(live);
    let reopened = reopen(root, home, &runs);
    assert!(reopened.offer.is_none(), "a reopen offers nothing again");
    assert_eq!(reopened.recorded, after.recorded, "kept as recorded");
}

/// The opinion waiting, after the person's request, the save and run in words, the run, the
/// adjustment picked by its identity, the revision proposed, its save and run in words and its
/// run, each acted as the shell routes it (`routed`, `picked`) through `Said`'s own doors.
fn earn_opinion(live: &mut Live, busy: &mpsc::Sender<String>, seen: &Seen) -> Waiting {
    let free = painted(&live.open());
    let proposed = routed(&free, REQUEST.to_owned()).send(live, busy, seen);
    let consent = painted(&proposed.beats);
    let saved = routed(&consent, SAVE_AND_RUN.to_owned()).send(live, busy, seen);
    let handoff = saved
        .handoff
        .expect("the save and run hands the first run over");
    let adjust = offered(&live.perform(&handoff), "adjust");
    let presentation = picked(&adjust, r#"["presentation"]"#.to_owned());
    let amended = presentation.send(live, busy, seen);
    let revision = painted(&amended.beats);
    let rerun = routed(&revision, SAVE_AND_RUN.to_owned()).send(live, busy, seen);
    let handoff = rerun
        .handoff
        .expect("the revision's save and run hands its run over");
    offered(&live.perform(&handoff), "opinion")
}

/// The genuine opinion's lineage, read from the Session: one record, the adjustment of the
/// presentation; the offer an opinion following that answer, about a later revision of the same
/// request, its own bytes, execution and the note it wrote.
fn lineage(earned: &Feedback) -> FeedbackOffer {
    let offer = earned.offer.clone().expect("a genuine opinion waits");
    let [adjusted] = earned.recorded.as_slice() else {
        panic!("one record, the adjustment: {:?}", earned.recorded)
    };
    assert_eq!(adjusted.kind, FeedbackKind::Adjust);
    assert_eq!(adjusted.picked, ["presentation"]);
    assert_eq!(offer.kind, FeedbackKind::Opinion);
    assert_eq!(offer.follows.as_deref(), Some(adjusted.question.as_str()));
    assert_eq!(offer.scope.request, adjusted.scope.request);
    assert!(offer.scope.revision > adjusted.scope.revision, "{offer:?}");
    assert_ne!(offer.scope.source_sha256, adjusted.scope.source_sha256);
    assert_ne!(offer.scope.execution, adjusted.scope.execution);
    let paths: Vec<&str> = (offer.scope.artifacts.iter())
        .map(|artifact| artifact.path.as_str())
        .collect();
    assert_eq!(paths, [NOTE]);
    offer
}

/// What `Enter` leaves as after `typed` in the own reply of the choice `waiting` paints (a row
/// first selected with `Down` when `down`): the composer's act, as what it was sent, and the
/// line the shell makes of it (the native pick, else the words routed).
fn enter_after(
    waiting: &Waiting,
    typed: &str,
    down: bool,
) -> (Offered, Option<Sent>, Option<Said>) {
    let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(waiting);
    for c in typed.chars() {
        composer.own_key(press(KeyCode::Char(c)));
    }
    if down {
        composer.offer_key(waiting, press(KeyCode::Down));
    }
    let sent = composer.offer_key(waiting, press(KeyCode::Enter));
    let said = match &sent {
        Offered::Answer(text) if composer.in_flight() == Some(Sent::Offer) => {
            Some(picked(waiting, text.clone()))
        }
        Offered::Answer(text) => Some(routed(waiting, text.clone())),
        _ => None,
    };
    (sent, composer.in_flight(), said)
}

/// What the last beat of a turn paints as waiting.
fn painted(beats: &[Beat]) -> Waiting {
    (beats.iter().rev())
        .find_map(|beat| match beat {
            Beat::Wait(waiting) => Some(waiting.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a turn ends painting what waits: {beats:?}"))
}

/// The offer about the last result of `kind`, painted as the native action the beats end on.
fn offered(beats: &[Beat], kind: &str) -> Waiting {
    let waiting = painted(beats);
    assert!(
        matches!(&waiting, Waiting::Action { key, .. } if key == kind),
        "the offer about the result, {kind}: {waiting:?}"
    );
    waiting
}

/// The identity the action `waiting` was painted with.
fn witness(waiting: &Waiting) -> String {
    let Waiting::Action { asked, .. } = waiting else {
        panic!("a native action: {waiting:?}")
    };
    asked.witness.clone()
}

/// What the Session keeps of the person's views of results, now.
fn feedback(live: &Live) -> Feedback {
    (live.session_for_tests())
        .expect("the session")
        .work()
        .feedback
}

/// The conversation opened again on the same project and home: what it kept.
fn reopen(root: &Path, home: &Path, runs: &Arc<AtomicUsize>) -> Feedback {
    let mut again = live(root, home, runs);
    let _ = again.open();
    feedback(&again)
}

fn sinks() -> (mpsc::Sender<String>, Seen) {
    let (busy, _) = mpsc::channel();
    let (queue, _) = mpsc::sync_channel(64);
    (busy, Seen::new(queue, Arc::new(Gap::default())))
}

/// The conversation a host opens on the person's local route, the history kept under `home`,
/// each run ended as a real one would leave it (`ran`).
fn live(root: &Path, home: &Path, runs: &Arc<AtomicUsize>) -> Live {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("vllm".to_owned());
    let local = IntelligenceKind::Local {
        provider: "vllm".to_owned(),
    };
    let pref = UserIntelligencePreference::new(local, Some(SEAT.to_owned()));
    let factory = Box::new(
        |resolved: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
            match (&resolved.kind, &resolved.model) {
                (IntelligenceKind::Local { provider }, Some(model)) => Box::new(ProviderReasoner {
                    model: model.clone(),
                    label: format!("{provider} · local"),
                }),
                _ => Box::new(NoReasoner),
            }
        },
    );
    let counted = Arc::clone(runs);
    let runners = Runners {
        run_once: Box::new(move |root: &Path, request: &RunRequest| ran(root, request, &counted)),
        run_resume: Box::new(|_, _, _, _| panic!("no resume in this bridge")),
        run_tapped: None,
    };
    Live::new(
        root.to_path_buf(),
        census,
        Some(pref),
        Some(home.to_path_buf()),
        factory,
        runners,
    )
}

/// The run of the requested workflow, as a real one leaves it: the note written in the project
/// (the first run's words, then the amended run's), and its journal, one execution on every frame,
/// its start naming the hash of the bytes the Session requested to run.
fn ran(root: &Path, request: &RunRequest, runs: &AtomicUsize) -> (u8, Option<PathBuf>) {
    let k = runs.fetch_add(1, Ordering::SeqCst);
    let bytes = std::fs::read(root.join(&request.workflow)).expect("the requested workflow");
    let words = if k == 0 { FIRST } else { SECOND };
    std::fs::write(root.join(NOTE), words).expect("the note");
    let at = PathBuf::from(format!(".nika/traces/bridge-{k}.ndjson"));
    let trace = root.join(&at);
    std::fs::create_dir_all(trace.parent().expect("traces")).expect("the traces directory");
    std::fs::write(&trace, journal(&sha256_hex(&bytes), k)).expect("the trace");
    (0, Some(at))
}

/// The frames of a run that completed writing the note.
fn journal(started: &str, k: usize) -> String {
    let execution = json!({"uuid": format!("01a10897-52a2-7758-aa31-{k:012}")});
    let frame = |kind: &str, fields: &[(&str, Value)]| {
        let fields: Vec<Value> = (fields.iter())
            .map(|(key, value)| json!({"key": key, "value": value}))
            .collect();
        json!({"kind": kind, "execution": execution, "fields": fields}).to_string()
    };
    let task = json!("write_note");
    let lines = [
        frame(
            "workflow_started",
            &[
                ("workflow", json!("hello-note")),
                ("workflow_sha256", json!(started)),
            ],
        ),
        frame(
            "task_started",
            &[
                ("task", task.clone()),
                ("note", json!("invoke · nika:write")),
            ],
        ),
        frame(
            "permit_checked",
            &[
                ("task", task.clone()),
                ("plane", json!("fs")),
                ("gate", json!(format!("permits.fs.write {NOTE}"))),
                ("decision", json!("allow")),
            ],
        ),
        frame(
            "task_completed",
            &[
                ("task", task),
                ("outcome", json!(json!({"class": "success"}).to_string())),
                ("output", json!(json!(NOTE).to_string())),
            ],
        ),
        frame("workflow_completed", &[("status", json!("succeeded"))]),
    ];
    lines.join("\n") + "\n"
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The candidate: one task writing the note's words.
fn note(words: &str) -> String {
    format!(
        "nika: hello-note\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"{NOTE}\"]\ntasks:\n  write_note:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{NOTE}\", content: \"{words}\" }} }}\n"
    )
}

/// What the scripted author does, in order: the candidate and its proposal; Save and Run on the
/// person's words `u2`; after the adjustment, the revision and its proposal; Save and Run on the
/// person's words `u3`; a reply to anything said after.
fn author() -> VecDeque<Step> {
    let run_on = |message: &str| {
        json!({"acts": ["save", "run"],
            "authorized_by": {"message": message, "excerpt": SAVE_AND_RUN_WORDS}})
    };
    VecDeque::from([
        Step::Call(
            "candidate_write",
            json!({"source": note("bonjour"), "resolutions": [],
                "summary": "écrit bonjour dans ./hello.md"}),
            None,
        ),
        Step::Call(
            "propose",
            json!({}),
            Some("Voici le workflow : il écrit bonjour dans ./hello.md."),
        ),
        Step::Call(
            "propose",
            run_on("u2"),
            Some("C'est enregistré ; je le lance."),
        ),
        Step::Call(
            "candidate_write",
            json!({"source": note("# bonjour"), "resolutions": [],
                "summary": "bonjour en titre Markdown"}),
            None,
        ),
        Step::Call(
            "propose",
            json!({}),
            Some("Voici la révision : bonjour en titre Markdown."),
        ),
        Step::Call(
            "propose",
            run_on("u3"),
            Some("Je l'enregistre et je la relance."),
        ),
        Step::Say("D'accord."),
    ])
}

/// One case in a child process against its own peer: `Err` with the child's last words when it
/// failed or did not finish.
fn run_child(case: &str, typed: &str) -> Result<(), String> {
    let scratch = Scratch::new(case);
    let (root, home) = (scratch.0.join("project"), scratch.0.join("home"));
    std::fs::create_dir_all(&root).expect("the project");
    std::fs::create_dir_all(&home).expect("the home");
    let peer = Peer::start(author(), VecDeque::from(["save_and_run", "save_and_run"]));
    let log = scratch.0.join("child.log");
    let out = std::fs::File::create(&log).expect("the child's log");
    let err = out.try_clone().expect("the child's log");
    let mut process = Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "app::said::bridge_tests::child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.as_os_str())
        .env("TMPDIR", std::env::temp_dir())
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("NIKA_VLLM_BASE_URL", peer.base())
        .env("BRIDGE_CASE", case)
        .env("BRIDGE_TYPED", typed)
        .env("BRIDGE_ROOT", root.as_os_str())
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .expect("the child");
    let status = finished(&mut process);
    peer.shutdown();
    if matches!(status, Some(code) if code.success()) {
        return Ok(());
    }
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    let last: Vec<&str> = said.lines().rev().take(16).collect();
    Err(format!(
        "{case} {typed:?} ({status:?}; the peer answered {:?}):\n{}",
        peer.kinds(),
        last.into_iter().rev().collect::<Vec<_>>().join("\n")
    ))
}

/// The child's exit, or `None` once it ran past two minutes and was stopped.
fn finished(process: &mut std::process::Child) -> Option<ExitStatus> {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = process.try_wait().expect("the child's status") {
            return Some(status);
        }
        if Instant::now() > deadline {
            let _ = process.kill();
            let _ = process.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A scratch folder under the temporary directory, removed when the case ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(case: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nika-feedback-bridge-{case}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("the scratch folder");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One reply of the scripted author: a tool call with the words beside it, or words that end
/// its turn.
enum Step {
    Call(&'static str, Value, Option<&'static str>),
    Say(&'static str),
}

/// The kinds of request the peer tells apart.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A turn of the author agent: the request offers tools.
    Agent,
    /// The verifier's closed choice, approved.
    Judge,
    /// A one-shot document round, which gets no document.
    Document,
    /// The bounded reading of a reply.
    Reading,
    /// The Session's reading of which offer an answer picks.
    Pick,
    /// Anything else: a plain reply.
    Reply,
}

/// The loopback author on the OpenAI-compatible wire: what it answered, by kind.
struct Peer {
    port: u16,
    kinds: Arc<Mutex<Vec<Kind>>>,
    stop: Arc<Mutex<bool>>,
}

impl Peer {
    /// The peer answering the author's `agent` steps and the Session's `picks` readings in order.
    fn start(agent: VecDeque<Step>, picks: VecDeque<&'static str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("its address").port();
        let kinds = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(Mutex::new(false));
        let (kept, halt) = (Arc::clone(&kinds), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut queues = Queues {
                agent,
                picks,
                calls: 0,
            };
            for stream in listener.incoming() {
                if *halt.lock().expect("the stop flag") {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_request(&mut stream) else {
                    continue;
                };
                let kind = kind_of(&body);
                let answer = queues.answer(kind, &body);
                kept.lock().expect("the kinds").push(kind);
                respond(&mut stream, &answer);
            }
        });
        Self { port, kinds, stop }
    }

    /// The base URL a local engine's override takes (`NIKA_VLLM_BASE_URL`).
    fn base(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn kinds(&self) -> Vec<Kind> {
        self.kinds.lock().expect("the kinds").clone()
    }

    fn shutdown(&self) {
        *self.stop.lock().expect("the stop flag") = true;
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The scripts the peer answers from, consumed in order.
struct Queues {
    agent: VecDeque<Step>,
    picks: VecDeque<&'static str>,
    calls: usize,
}

impl Queues {
    fn answer(&mut self, kind: Kind, body: &Value) -> String {
        match kind {
            Kind::Agent => self.agent_turn(),
            Kind::Judge => judged(body),
            Kind::Document => text_completion(
                &json!({"candidate": "", "candidate_lines": [], "operations": [],
                    "questions": [], "gaps": [], "notes": "no document"})
                .to_string(),
            ),
            Kind::Reading => text_completion("NONE"),
            Kind::Pick => text_completion(self.picks.pop_front().unwrap_or("NONE")),
            Kind::Reply => text_completion("D'accord."),
        }
    }

    fn agent_turn(&mut self) -> String {
        self.calls += 1;
        match self.agent.pop_front() {
            Some(Step::Call(name, args, text)) => completion(
                &json!({"role": "assistant", "content": text, "tool_calls": [{
                    "id": format!("call_{}", self.calls), "type": "function",
                    "function": {"name": name, "arguments": args.to_string()}}]}),
                "tool_calls",
            ),
            Some(Step::Say(text)) => text_completion(text),
            None => text_completion("(the scripted author has nothing more to say)"),
        }
    }
}

/// The verifier's closed choice, approved: its approving option, else « unlocated »; with no
/// model-written part where its schema asks for them.
fn judged(body: &Value) -> String {
    let schema = &body["response_format"]["json_schema"]["schema"];
    let keys = schema["properties"]["choice"]["enum"].as_array();
    let offers = |key: &str| keys.is_some_and(|keys| keys.iter().any(|k| k == key));
    let approve = APPROVALS.into_iter().find(|a| offers(a));
    let choice = approve
        .or_else(|| offers(UNLOCATED).then_some(UNLOCATED))
        .unwrap_or("none");
    let mut answer = json!({"choice": choice});
    if !schema["properties"]["model_parts"].is_null() {
        answer["model_parts"] = json!([]);
    }
    text_completion(&answer.to_string())
}

/// The kind of a request, read from its body.
fn kind_of(body: &Value) -> Kind {
    if body["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty())
    {
        return Kind::Agent;
    }
    let properties = &body["response_format"]["json_schema"]["schema"]["properties"];
    let judge = |k: &Value| k == UNLOCATED || APPROVALS.iter().any(|a| k == a);
    if (properties["choice"]["enum"].as_array()).is_some_and(|keys| keys.iter().any(judge)) {
        return Kind::Judge;
    }
    if properties.get("candidate").is_some() {
        return Kind::Document;
    }
    let last = (body["messages"].as_array().into_iter().flatten())
        .next_back()
        .and_then(|message| message["content"].as_str())
        .unwrap_or_default();
    if last.contains("asked a human for one value") || last.contains("asked a human to choose") {
        return Kind::Reading;
    }
    if last.contains("One answer on one line, nothing else.") {
        return Kind::Pick;
    }
    Kind::Reply
}

fn completion(message: &Value, finish: &str) -> String {
    json!({"id": "chatcmpl-feedback-bridge", "object": "chat.completion", "created": 0,
        "model": "oux-author",
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200}})
    .to_string()
}

fn text_completion(text: &str) -> String {
    completion(&json!({"role": "assistant", "content": text}), "stop")
}

fn read_request(stream: &mut TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok()?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_lowercase();
    let length: usize = (head.lines())
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    while buffer.len() < header_end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&buffer[header_end..header_end + length]).ok()
}

fn respond(stream: &mut TcpStream, body: &str) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}
