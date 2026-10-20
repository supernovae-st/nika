// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A reply bound to one question, through the host: it reaches the question its identity names
//! only when the current snapshot asks it, through the Session's identity door; its command
//! identity binds the question with the line; and it is refused before the Session for a stale
//! snapshot, a question an earlier snapshot asked, or one this Session never asked. The real
//! Session over a temporary project, its reasoner never asked (the compiler asks the model).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nika_session::outcome::Incarnation;
use nika_session::work::{
    AskedQuestion, Feedback, FeedbackArtifact, FeedbackKind, FeedbackOffer, FeedbackScope, Offer,
    Rail, Request,
};
use nika_session::{Lifecycle, LifecycleFacts};
use serde_json::{Value, json};

use super::tests::{DRAFT, runtime, world};
use super::*;
use crate::run::NoRunDoor;

fn start(root: &Path) -> SessionHost {
    SessionHost::start(runtime(root), Box::new(NoRunDoor::new("none")), Vec::new()).expect("host")
}

fn frame(frame: &Frame) -> Value {
    serde_json::to_value(frame).expect("frame")
}

/// The current snapshot, as a read answers it.
fn current(host: &SessionHost) -> Value {
    frame(&host.snapshot())["snapshot"].clone()
}

fn handle(snapshot: &Value) -> String {
    snapshot["snapshot"].as_str().expect("handle").to_owned()
}

/// `command` accepted, and its settled result.
fn settled(host: &SessionHost, command: Command) -> Value {
    match host.dispatch(command) {
        Dispatch::Accepted { command, .. } => frame(&host.wait_result(&command).expect("settled")),
        other => panic!("not accepted: {other:?}"),
    }
}

/// `command` answered at once: a refusal or a replayed result.
fn direct(host: &SessionHost, command: Command) -> Value {
    match host.dispatch(command) {
        Dispatch::Reply(reply) => frame(&reply),
        other => panic!("not a direct reply: {other:?}"),
    }
}

fn submit(command: &str, snapshot: &str, line: &str) -> Command {
    Command::Submit {
        command: command.to_owned(),
        snapshot: snapshot.to_owned(),
        line: line.to_owned(),
    }
}

fn answer(command: &str, snapshot: &str, question: &str, line: &str) -> Command {
    Command::Answer {
        command: command.to_owned(),
        snapshot: snapshot.to_owned(),
        question: question.to_owned(),
        line: line.to_owned(),
    }
}

fn delegate(command: &str, snapshot: &str, question: &str) -> Command {
    Command::Delegate {
        command: command.to_owned(),
        snapshot: snapshot.to_owned(),
        question: question.to_owned(),
    }
}

/// The compiler's question, named by its identity: refused before the Session for an earlier
/// snapshot or an identity this Session never asked, the question still waiting; answered through
/// its identity door, the act naming it; that command replayed from its record, other bytes under
/// it a conflict; and named again once answered, refused as asked by an earlier snapshot.
#[test]
fn an_answer_reaches_only_the_question_the_current_snapshot_asks() {
    let root = world();
    let host = start(root.path());
    let opened = handle(&current(&host));
    let asked = settled(&host, submit("c-1", &opened, DRAFT));
    let question = asked["snapshot"]["work"]["waiting"]["id"]
        .as_str()
        .expect("the question's identity")
        .to_owned();
    let shown = handle(&asked["snapshot"]);
    let stale = direct(&host, answer("c-2", &opened, &question, "mock/echo"));
    assert_eq!(stale["error"], "stale_snapshot", "{stale}");
    let never = direct(&host, answer("c-2", &shown, &"0".repeat(64), "mock/echo"));
    assert_eq!(never["error"], "unknown_question", "{never}");
    assert_eq!(
        never["line"], "mock/echo",
        "the refused line is returned whole"
    );
    assert_eq!(current(&host)["work"]["waiting"]["id"], question.as_str());
    assert_eq!(
        handle(&current(&host)),
        shown,
        "nothing reached the Session"
    );
    let bound = settled(&host, answer("c-2", &shown, &question, "mock/echo"));
    assert_eq!(bound["op"], "answer", "{bound}");
    assert_eq!(
        bound["snapshot"]["work"]["answered"],
        json!({"question": question, "act": "bound", "key": "model",
            "value": "mock/echo", "reading": "as_typed"})
    );
    let replay = direct(&host, answer("c-2", &shown, &question, "mock/echo"));
    assert_eq!(replay["replayed"], true, "{replay}");
    assert_eq!(replay["event"], bound["event"]);
    let conflict = direct(&host, answer("c-2", &shown, &question, "mock/other"));
    assert_eq!(conflict["error"], "command_conflict", "{conflict}");
    let elsewhere = direct(&host, delegate("c-2", &shown, &question));
    assert_eq!(
        elsewhere["error"], "command_conflict",
        "another op under that identity"
    );
    let now = handle(&current(&host));
    let again = direct(&host, answer("c-3", &now, &question, "mock/echo"));
    assert_eq!(again["error"], "unknown_question", "{again}");
    let words = again["message"].as_str().unwrap_or_default();
    assert!(words.contains("earlier snapshot"), "{again}");
    let (events, _) = host.events_after(0);
    let accepted: Vec<Value> = (events.iter())
        .map(frame)
        .filter(|f| f["frame"] == "accepted")
        .map(|f| f["op"].clone())
        .collect();
    assert_eq!(
        accepted,
        [json!("submit"), json!("answer")],
        "one turn each, nothing else reached the Session"
    );
}

/// The compiler's question asks a value it cannot invent: leaving it to Nika is refused by the
/// Session, its words never bound as that value, and the question keeps waiting.
#[test]
fn the_compilers_question_offers_no_choice_to_leave() {
    let root = world();
    let host = start(root.path());
    let asked = settled(&host, submit("c-1", &handle(&current(&host)), DRAFT));
    let question = asked["snapshot"]["work"]["waiting"]["id"].clone();
    let shown = handle(&asked["snapshot"]);
    let left = settled(
        &host,
        delegate("c-2", &shown, question.as_str().expect("id")),
    );
    assert_eq!(left["op"], "delegate", "{left}");
    assert_eq!(left["outcomes"][0]["kind"], "refusal", "{left}");
    assert_eq!(left["outcomes"][0]["class"], "wrong_state", "{left}");
    let work = &left["snapshot"]["work"];
    assert_eq!(work["waiting"]["id"], question, "the question still waits");
    assert_eq!(
        work["answered"],
        json!({"question": question, "act": "refused", "class": "wrong_state"})
    );
}

/// A snapshot waiting on `waiting`, showing `feedback`.
fn showing(waiting: Waiting, feedback: Feedback) -> Work {
    let rail = Rail::from(&Lifecycle::from_facts(&LifecycleFacts::new()));
    Work::new(
        PathBuf::from("/fixture"),
        Request::default(),
        waiting,
        None,
        None,
        None,
        None,
        rail,
    )
    .with_feedback(feedback)
}

/// The optional offer about a result is a question a reply may name only while the Session waits
/// on a free line, where the Session takes it too; a question that owns the next line keeps it,
/// and a free prompt with no offer asks nothing.
#[test]
fn the_offer_about_a_result_is_named_only_at_the_free_prompt() {
    let incarnation = Arc::new(Incarnation);
    let offered = QuestionId::new("feedback".into(), &incarnation);
    let options = vec![
        Offer::new("depth", "Depth of the summary", false),
        Offer::new("later", "Later", false).exclusive(),
    ];
    let question = AskedQuestion::new(offered.clone(), "adjust", "What would you like to adjust?")
        .with_options(options, true, true);
    let artifact = FeedbackArtifact::new("news/digest.md", "ab".repeat(32));
    let scope = FeedbackScope::new((0, 1), "s1", "run-1", vec![artifact]);
    let offer = FeedbackOffer::new(FeedbackKind::Adjust, question, scope, None);
    let feedback = Feedback::new(Some(offer), Vec::new());

    let free = showing(Waiting::Free, feedback.clone());
    assert_eq!(asked(&Waiting::Free, &free), [offered]);

    let owner = QuestionId::new("q1".into(), &incarnation);
    let owning = Waiting::Question {
        key: "sources".into(),
        id: owner.clone(),
    };
    let asking = showing(owning.clone(), feedback);
    assert_eq!(
        asked(&owning, &asking),
        [owner],
        "the question that owns the next line, never the offer"
    );

    let bare = showing(Waiting::Free, Feedback::default());
    assert!(
        asked(&Waiting::Free, &bare).is_empty(),
        "no offer, no identity"
    );
}
