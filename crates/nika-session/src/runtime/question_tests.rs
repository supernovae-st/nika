// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The question that takes the next line travels typed in the work snapshot every host reads:
//! the compiler's own question document, beside the unchanged identity an answer names, and
//! nothing while another prompt owns the line or no question waits.

use std::path::Path;

use nika_onboard::compile::{
    CompileRequest, QuestionType, compile, intent_sha256, outcome_document,
};
use serde_json::{Value, json};

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
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
    let none = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: false,
        why: None,
    };
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
