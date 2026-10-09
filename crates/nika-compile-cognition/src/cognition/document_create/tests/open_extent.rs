// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An open name through the document door, then its answer round: the stock words leave the first
//! word of `un payload out/notification.json` open (a noun before a path, or a spaced folder), and
//! Law 1 owes it exactly. SCRIPTED seats over the real compile entry establish the door's
//! mechanics, never a model's behaviour. A document writing the shorter file is refused, the
//! finding naming the name and how the human settles it; the repair asks it, naming it in its
//! label, and is accepted with no candidate while the closed choice of its readings is asked. The
//! answer round replays that recorded document with no author call: the answer bakes into its
//! constant, the one empty write entry is completed beside the report's grant, every other byte
//! is the author's, and only the round's judge makes it READY.

use super::{Scripted, calls, compiled_with, door, policy};
use crate::QuestionType;
use serde_json::json;

/// The stock request's sentences around its payload, verbatim.
const STOCK: &str = "Lis world/source.json. Écris out/report.json avec les alertes. Une fois la source complète, si des alertes certaines existent, prépare exactement un payload out/notification.json contenant channel=\"stock\" et item_ids dans leur ordre, puis effectue exactement un POST /notifications/stock vers le sink local fourni par le futur pilote. Pour cette exécution réelle, le pilote a démarré le sink local http://127.0.0.1:57468 ; effectue le POST prévu, uniquement dans les conditions ci-dessus.";

/// The open name, owed exactly unless the human settles it.
const PAYLOAD: &str = "payload out/notification.json";

/// The author's document: the source read and parsed, the alert ids computed, the report and the
/// payload written (`@PATH@`, with `@CONST@` and the write side `@WRITE@`), the POST sent.
const DOCUMENT: &str = r#"nika: stock-alerts
@CONST@permits:
  fs:
    read: ["world/source.json"]
    write: @WRITE@
  net:
    http: ["127.0.0.1"]
  tools: ["nika:read", "nika:jq", "nika:write", "nika:fetch"]
tasks:
  read_source:
    invoke: { tool: "nika:read", args: { path: "world/source.json" } }
  alerts:
    with: { raw: "${{ tasks.read_source.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson | [.items[] | select(.stock < .threshold) | .id]" } }
  write_report:
    with: { ids: "${{ tasks.alerts.output }}" }
    invoke: { tool: "nika:write", args: { path: "out/report.json", content: "${{ with.ids }}" } }
  write_payload:
    with: { ids: "${{ tasks.alerts.output }}" }
    invoke: { tool: "nika:write", args: { path: "@PATH@", content: "${{ with.ids }}" } }
  post:
    after: { write_payload: success }
    with: { ids: "${{ tasks.alerts.output }}" }
    invoke: { tool: "nika:fetch", args: { url: "http://127.0.0.1:57468/notifications/stock", method: POST, body: "${{ with.ids }}" } }
"#;

/// The document writing the shorter file itself, as an author reading the words would.
fn shorter() -> String {
    (DOCUMENT.replace("@CONST@", ""))
        .replace("@WRITE@", r#"["out/report.json", "out/notification.json"]"#)
        .replace("@PATH@", "out/notification.json")
}

/// The document asking the payload's file: a blank `const.payload_path`, read whole as that
/// task's path, one empty write entry beside the report's grant.
fn asking() -> String {
    (DOCUMENT.replace("@CONST@", "const:\n  payload_path: \"\"\n"))
        .replace("@WRITE@", r#"["out/report.json", ""]"#)
        .replace("@PATH@", "${{ const.payload_path }}")
}

/// The author's answer asking `const.payload_path` under `label`.
fn asked(candidate: &str, label: &str) -> String {
    let question = json!({"key": "const.payload_path", "label": label, "answer_type": "text",
        "why": "Les mots de la requête laissent ouvert le début du nom du fichier."});
    json!({"candidate": candidate, "candidate_lines": [], "operations": [],
        "questions": [question], "gaps": [], "notes": "scripted"})
    .to_string()
}

/// The diagnostics a round of the outcome's journal recorded, as text.
fn found(out: &crate::CompileOutcome, round: usize) -> String {
    let decision = out.provenance.decision.as_ref().expect("a decision");
    decision["native"]["rounds"][round]["diagnostics"].to_string()
}

/// The authoring calls the outcome's receipt journals.
fn authored(out: &crate::CompileOutcome) -> Vec<String> {
    (calls(out).into_iter())
        .filter(|call| call.starts_with("document"))
        .collect()
}

#[tokio::test]
async fn an_open_name_is_asked_then_answered_on_the_same_recorded_document() {
    let label = format!("Quel fichier désigne « {PAYLOAD} » ?");
    let author = Scripted::new(vec![door(&shorter(), &[]), asked(&asking(), &label)]);
    let request = crate::CompileRequest::create(STOCK).with_authoring_policy(policy());
    let out = compiled_with(&request, &author, None, None).await;
    assert_eq!(authored(&out), ["document", "document-repair"], "{out:#?}");
    let refused = found(&out, 0);
    let owed = format!("UNREALIZED PATH: the request names `{PAYLOAD}`");
    assert!(refused.contains(&owed), "{refused}");
    assert!(refused.contains("the human settles it"), "{refused}");
    assert!(
        !refused.contains("`/notifications/stock`"),
        "the route is sent: {refused}"
    );
    assert_eq!(found(&out, 1), "[]", "the asked document passes the laws");
    assert_eq!(out.status, crate::CompileStatus::Incomplete, "{out:#?}");
    assert!(
        out.candidate.is_none(),
        "nothing is emitted while it is asked"
    );
    let question = (out.questions.iter()).find(|q| q.key == "const.payload_path");
    let question = question.expect("the open name is asked");
    assert!(question.mandatory && question.answer_type == QuestionType::Choice);
    let offers: Vec<&str> = question.options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(offers, [PAYLOAD, "out/notification.json"]);
    let record = out.provenance.plan.clone().expect("the recorded document");
    assert_eq!(record["source"], json!(asking()));
    assert_eq!(record["questions"][0]["answer_type"], "choice");
    // The answer round: the same recorded document, judged; no author is asked again.
    let answered = (request.clone().with_plan(record))
        .answer("const.payload_path", "\"out/notification.json\"");
    let judge = Scripted::new(Vec::new());
    let done = compiled_with(&answered, &judge, None, None).await;
    assert!(authored(&done).is_empty(), "{:?}", calls(&done));
    let expected = (asking().replace(
        "  payload_path: \"\"\n",
        "  payload_path: \"out/notification.json\"\n",
    ))
    .replace(
        "    write: [\"out/report.json\", \"\"]\n",
        "    write: [\"out/report.json\",\"out/notification.json\"]\n",
    );
    assert_eq!(
        done.candidate.as_deref(),
        Some(expected.as_str()),
        "{done:#?}"
    );
    assert_eq!(done.status, crate::CompileStatus::Ready, "{done:#?}");
    // With no judge in the round, the same bytes wait for one: nothing is READY by itself.
    let keyless = nika_compile::compile(&answered).expect("compiles");
    assert_eq!(keyless.candidate, done.candidate);
    assert_eq!(keyless.status, crate::CompileStatus::Incomplete);
    assert!(
        (keyless.diagnostics.iter()).any(|d| d.target == "semantic_verification"),
        "{keyless:#?}"
    );
}

/// A question that does not name the open name binds nothing: the name stays owed exactly in
/// every round, the author is told how to ask it, and the unnamed question is never asked.
#[tokio::test]
async fn a_question_naming_no_open_name_leaves_it_owed() {
    let unnamed = asked(&asking(), "Où écrire le payload ?");
    let author = Scripted::new(vec![unnamed.clone(), unnamed]);
    let request = crate::CompileRequest::create(STOCK).with_authoring_policy(policy());
    let out = compiled_with(&request, &author, None, None).await;
    let owed = format!("UNREALIZED PATH: the request names `{PAYLOAD}`");
    for round in 0..2 {
        let said = found(&out, round);
        assert!(
            said.contains(&owed) && said.contains("the human settles it"),
            "{said}"
        );
    }
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_ne!(out.status, crate::CompileStatus::Ready);
    assert!(
        !(out.questions.iter()).any(|q| q.key == "const.payload_path"),
        "{out:#?}"
    );
}

/// A request with two open names: a source and a payload.
const TWO: &str = "Lis un journal in/a.json. Écris out/report.json, puis prépare exactement un payload out/c.json.";

/// One constant read and written whole, asked under one key twice, each question naming the
/// other open name.
const ONE_KEY: &str = r#"nika: journal-payload
const:
  location_path: ""
permits:
  fs:
    read: [""]
    write: ["out/report.json", ""]
  tools: ["nika:read", "nika:write"]
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "${{ const.location_path }}" } }
  write_report:
    with: { text: "${{ tasks.load.output }}" }
    invoke: { tool: "nika:write", args: { path: "out/report.json", content: "${{ with.text }}" } }
  write_payload:
    with: { text: "${{ tasks.load.output }}" }
    invoke: { tool: "nika:write", args: { path: "${{ const.location_path }}", content: "${{ with.text }}" } }
"#;

/// One key never answers two open names: through the door, both names stay owed in every round,
/// no candidate is emitted and no closed choice of their readings is offered.
#[tokio::test]
async fn one_key_asked_for_two_open_names_is_never_accepted() {
    let question = |label: &str| {
        json!({"key": "const.location_path", "label": label, "answer_type": "text",
            "why": "Les mots de la requête laissent ouvert le début du nom du fichier."})
    };
    let questions = [
        question("Quel fichier est « journal in/a.json » ?"),
        question("Quel fichier est « payload out/c.json » ?"),
    ];
    let answer = json!({"candidate": ONE_KEY, "candidate_lines": [], "operations": [],
        "questions": questions, "gaps": [], "notes": "scripted"})
    .to_string();
    let author = Scripted::new(vec![answer.clone(), answer]);
    let request = crate::CompileRequest::create(TWO).with_authoring_policy(policy());
    let out = compiled_with(&request, &author, None, None).await;
    assert_eq!(authored(&out), ["document", "document-repair"], "{out:#?}");
    for round in 0..2 {
        let said = found(&out, round);
        for name in ["journal in/a.json", "payload out/c.json"] {
            let owed = format!("UNREALIZED PATH: the request names `{name}`");
            assert!(said.contains(&owed), "round {round}, {name}: {said}");
        }
    }
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_ne!(out.status, crate::CompileStatus::Ready);
    assert!(
        (out.questions.iter()).all(|q| q.key != "const.location_path" && q.options.is_empty()),
        "{out:#?}"
    );
}
