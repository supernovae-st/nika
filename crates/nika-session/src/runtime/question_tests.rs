// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The question that takes the next line travels typed in the work snapshot every host reads:
//! the compiler's own question document, beside the unchanged identity an answer names, and
//! nothing while another prompt owns the line or no question waits. What the last line did to
//! the question it was typed for travels beside it, recorded where the session did it.

use std::path::Path;

use nika_onboard::compile::{
    CompileRequest, QuestionType, compile, intent_sha256, outcome_document,
};
use serde_json::{Value, json};

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::outcome::{Incarnation, QuestionId};
use crate::reasoner::NoReasoner;

const INTENT: &str = "Copy entree.txt to the destination I pick.";

/// A native seat's candidate whose destination is a declared placeholder the round asks.
const SEAT: &str = r#"nika: copy-to-picked-file
const:
  destination_path: ""
permits:
  fs:
    read: ["./entree.txt"]
    write: [""]
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: { path: "./entree.txt" }
  write_destination:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "${{ const.destination_path }}", content: "${{ with.text }}" }
"#;

fn session(root: &Path) -> SessionRuntime {
    let none = ResolvedSessionIntelligence::new(
        IntelligenceKind::None,
        None,
        DataLocus::None,
        false,
        None,
    );
    SessionRuntime::open(root, none, Box::new(NoReasoner))
}

fn snapshot(s: &SessionRuntime) -> Value {
    serde_json::to_value(s.work()).expect("serializes")
}

/// The seat's record replayed (zero calls): its destination asked as a closed choice whose
/// options are not in their sorted order.
#[test]
fn the_waiting_question_is_the_compilers_own_beside_its_identity() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("entree.txt"), "A\n").expect("entree");
    let mut s = session(root.path());
    let record = json!({
        "strategy": "native",
        "intent_sha256": intent_sha256(INTENT),
        "source": SEAT,
        "questions": [{
            "key": "const.destination_path",
            "label": "Destination file",
            "answer_type": "choice",
            "why": "The request lets the user pick the destination.",
            "options": [
                {"key": "sortie.txt", "label": "The output file"},
                {"key": "archive.txt", "label": "The archive"}
            ]
        }],
        "gaps": [],
        "trigger": null,
    });
    let out = compile(&CompileRequest::create(INTENT).with_plan(record)).expect("replays");
    let mut round = AuthoringRound::new(INTENT);
    round.absorb(&out);
    s.authoring = Some(round);
    let asked = s.pending_question().expect("a question waits");
    assert_eq!(
        (asked.key.as_str(), asked.answer_type),
        ("const.destination_path", QuestionType::Choice)
    );
    let id = s.pending_question_id().expect("its identity");
    let document = outcome_document(&out);
    let compiled = (document["questions"].as_array().into_iter().flatten())
        .find(|q| q["key"] == "const.destination_path")
        .expect("the compiler's document asks it");

    let json = snapshot(&s);
    assert_eq!(
        json["waiting"],
        json!({"kind": "question", "key": "const.destination_path", "id": id.as_str()}),
        "the identity an answer names is unchanged"
    );
    assert_eq!(&json["question"], compiled, "{json}");
    assert_eq!(
        json["question"]["options"],
        json!([
            {"key": "sortie.txt", "label": "The output file"},
            {"key": "archive.txt", "label": "The archive"}
        ])
    );
    assert_eq!(
        (&json["question"]["type"], &json["question"]["mandatory"]),
        (&json!("choice"), &json!(true))
    );

    // The first screen owns the next line: the open question is not the one that takes it.
    s.pending_choice = true;
    let json = snapshot(&s);
    assert_eq!(json["waiting"], json!({"kind": "intelligence_choice"}));
    assert!(json.get("question").is_none(), "{json}");
    s.pending_choice = false;
    assert_eq!(&snapshot(&s)["question"], compiled);

    s.authoring = None;
    let json = snapshot(&s);
    assert_eq!(json["waiting"], json!({"kind": "free"}));
    assert!(json.get("question").is_none(), "{json}");
}

/// The seat's record replayed (zero calls) with `question` its one question: the round waits on
/// it, and its identity is returned.
fn ask(s: &mut SessionRuntime, question: &Value) -> QuestionId {
    let record = json!({
        "strategy": "native",
        "intent_sha256": intent_sha256(INTENT),
        "source": SEAT,
        "questions": [question],
        "gaps": [],
        "trigger": null,
    });
    let out = compile(&CompileRequest::create(INTENT).with_plan(record)).expect("replays");
    let mut round = AuthoringRound::new(INTENT);
    round.absorb(&out);
    s.authoring = Some(round);
    s.pending_question_id().expect("a question waits")
}

fn destination() -> Value {
    json!({
        "key": "const.destination_path",
        "label": "Destination file path",
        "answer_type": "text",
        "why": "The request lets the user pick the destination."
    })
}

/// What the last line did, as the work snapshot carries it; `None` when the key is absent.
fn answered(s: &SessionRuntime) -> Option<Value> {
    snapshot(s).get("answered").cloned()
}

fn refused_as(id: &QuestionId, class: &str) -> Value {
    json!({"question": id.as_str(), "act": "refused", "class": class})
}

/// Each line typed at a value question records what it did there, at the act: a sentence nothing
/// reads waits and says why, an aside and a read-only line record nothing, a command and an empty
/// line are refused, the value alone binds as typed and stays bound whatever the compile after it
/// says. Reading the work again changes nothing; the next line clears it first.
#[test]
fn each_line_at_a_question_records_what_it_did_there_and_a_new_line_clears_it() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("entree.txt"), "A\n").expect("entree");
    let mut s = session(root.path());
    let id = ask(&mut s, &destination());
    assert_eq!(answered(&s), None, "no line answered yet");

    let out = s.turn("mets-le dans le fichier de sortie");
    assert!(matches!(out, TurnOutcome::Question { .. }), "{out:?}");
    assert_eq!(
        s.pending_question_id().as_ref(),
        Some(&id),
        "the same question waits"
    );
    let waits = answered(&s).expect("the act");
    assert_eq!(
        (&waits["question"], &waits["act"], &waits["key"]),
        (
            &json!(id.as_str()),
            &json!("waits"),
            &json!("const.destination_path")
        )
    );
    assert!(
        (waits["why"].as_str()).is_some_and(|why| why.contains("nothing was bound")),
        "{waits}"
    );
    assert_eq!(
        answered(&s),
        Some(waits),
        "reading the work again is the same act"
    );

    assert!(matches!(s.turn("why?"), TurnOutcome::Aside(_)));
    assert_eq!(answered(&s), None, "an aside is no answer");
    let out = s.turn("/frobnicate");
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::WrongState),
        "{out:?}"
    );
    assert_eq!(answered(&s), Some(refused_as(&id, "wrong_state")));
    let out = s.turn("");
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::EmptyAnswer),
        "{out:?}"
    );
    assert_eq!(answered(&s), Some(refused_as(&id, "empty_answer")));
    assert_eq!(
        s.pending_question_id().as_ref(),
        Some(&id),
        "a refusal keeps it waiting"
    );

    // The compile that follows holds the built workflow: no model here can judge it.
    let out = s.turn("sortie.txt");
    assert!(
        matches!(&out, TurnOutcome::Facts(text) if text.starts_with("The workflow is built but not proposed")),
        "{out:?}"
    );
    assert!(s.pending_proposal().is_none() && matches!(s.waiting(), crate::work::Waiting::Free));
    let bound = json!({"question": id.as_str(), "act": "bound",
        "key": "const.destination_path", "value": "sortie.txt", "reading": "as_typed"});
    assert_eq!(answered(&s), Some(bound), "bound stays bound");
    // A line typed for a prompt that no longer waits answers nothing, and clears the act.
    let out = s.submit("x", &crate::work::Waiting::IntelligenceChoice);
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::StaleRevision),
        "{out:?}"
    );
    assert_eq!(answered(&s), None, "the next line clears it");
}

/// A choice answered by one offered key alone binds that key, and says so.
#[test]
fn an_offered_key_named_alone_binds_as_that_key() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("entree.txt"), "A\n").expect("entree");
    let mut s = session(root.path());
    let mut choice = destination();
    choice["answer_type"] = json!("choice");
    choice["options"] = json!([
        {"key": "sortie.txt", "label": "The output file"},
        {"key": "archive.txt", "label": "The archive"}
    ]);
    let id = ask(&mut s, &choice);
    let _ = s.turn("archive.txt");
    assert_eq!(
        answered(&s),
        Some(json!({"question": id.as_str(), "act": "bound",
            "key": "const.destination_path", "value": "archive.txt", "reading": "offered_key"}))
    );
}

/// A cancel drops the round and says which question it dropped; every later answer that names an
/// identity is refused under the identity it named — consumed, stale, or asked by another session
/// even with the very witness that waits — and the act names what the host named, never more.
#[test]
fn a_dropped_question_and_every_later_named_answer_record_the_identity_named() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("entree.txt"), "A\n").expect("entree");
    let mut s = session(root.path());
    let first = ask(&mut s, &destination());
    assert!(matches!(s.turn("cancel"), TurnOutcome::Facts(_)));
    assert_eq!(
        answered(&s),
        Some(
            json!({"question": first.as_str(), "act": "dropped", "key": "const.destination_path"})
        )
    );
    let out = s.answer_question_for(&first, "sortie.txt");
    assert!(matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::AlreadyConsumed));
    assert_eq!(
        answered(&s),
        Some(refused_as(&first, "already_consumed")),
        "the latest act wins"
    );

    s.questions.ask();
    let current = ask(&mut s, &destination());
    assert_ne!(current, first);
    let _ = s.answer_question_for(&first, "sortie.txt");
    assert_eq!(answered(&s), Some(refused_as(&first, "stale_revision")));
    let foreign = QuestionId::new(
        current.as_str().to_owned(),
        &std::sync::Arc::new(Incarnation),
    );
    let out = s.answer_question_for(&foreign, "sortie.txt");
    assert!(matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::StaleRevision));
    assert_eq!(
        answered(&s),
        Some(refused_as(&current, "stale_revision")),
        "the same witness on the wire, refused: a witness alone is no authority"
    );
    assert_eq!(
        s.pending_question_id().as_ref(),
        Some(&current),
        "it still waits"
    );
}
