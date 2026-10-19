// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

fn defect() -> Value {
    json!({"status":"defect","defects":[{"location":"r2","kind":"scope",
        "evidence":"Ask me now which greeting to use.",
        "note":"An already requested conversation choice is not a per-execution prompt."}]})
}

fn mistaken() -> Value {
    let mut result = reading();
    result["runtime_obligations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"r2",
        "requirement":"Ask for the greeting at every execution.","answer_keys":[],
        "evidence":[{"span":"request","quote":"Ask me now which greeting to use."}]}));
    result["preparation_requests"] = json!([]);
    result["coverage"][0]["runtime_ids"] = json!(["r1", "r2"]);
    result["coverage"][0]["preparation_ids"] = json!([]);
    result
}

#[tokio::test]
async fn audited_scope_feedback_repairs_the_reading_before_any_program_decision() {
    let author = Author::new([
        Some(mistaken()),
        Some(defect()),
        Some(reading()),
        Some(audit()),
    ]);
    let seat = Seat::new(false);
    let ready = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&author),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    assert_eq!(author.calls(), 4);
    let requests = author.requests.lock().unwrap();
    let state = |index: usize| {
        let ContentBlock::Text { text } = &requests[index].messages[1].content[0] else {
            panic!()
        };
        serde_json::from_str::<Value>(text).unwrap()
    };
    let source = input(INTENT, &request());
    assert_eq!(state(0), source);
    assert_eq!(
        state(2),
        json!({"source":source,"previous_interpretation":mistaken(),"repair_feedback":defect()})
    );
    assert_eq!(
        state(3),
        json!({"source":source,"interpretation":reading()})
    );
    let record = &ready.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(record["attempts"][0]["audit"], defect());
    assert_eq!(record["attempts"][1]["audit"], audit());
    assert_eq!(record["calls"].as_array().unwrap().len(), 4);
    let asked = seat.asked.lock().unwrap();
    let parts = asked
        .iter()
        .filter(|q| q.id.starts_with("verify-part-"))
        .collect::<Vec<_>>();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].state["obligation"]["requirement"], REQUIREMENT);
}

#[tokio::test]
async fn reordered_and_reworded_audit_notes_are_not_new_repair_progress() {
    let mut first = defect();
    first["defects"]
        .as_array_mut()
        .unwrap()
        .push(json!({"location":"r1","kind":"missing",
        "evidence":REQUIREMENT,"note":"The requested output is absent."}));
    let mut repeated = first.clone();
    repeated["defects"].as_array_mut().unwrap().reverse();
    for d in repeated["defects"].as_array_mut().unwrap() {
        d["location"] = json!("renamed-model-id");
        d["note"] = json!("A different paraphrase of the same problem.");
    }
    let mut renamed = mistaken();
    renamed["runtime_obligations"][1]["requirement"] = json!("Prompt for a greeting on each run.");
    let author = Author::new([Some(mistaken()), Some(first), Some(renamed), Some(repeated)]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let held = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&author),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(held.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 4);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(*room.0.lock().unwrap(), 0);
    let record = &held.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["repair_stop"]["reason"], "repeated_feedback");
    assert_eq!(record["attempts"].as_array().unwrap().len(), 2);
    assert!(held.diagnostics.iter().any(|finding| {
        finding.target == "request_interpretation"
            && finding.message.contains("stopped after repeated feedback")
            && finding
                .message
                .contains("Do not rewrite the workflow or repeat the same verification")
    }));
}

#[tokio::test]
async fn genuine_scope_ambiguity_stays_held_without_inventing_a_runtime_answer() {
    let words = "Write hello to ./out/result.txt. Approval may be needed now or at every execution; I have not decided which, so do not choose that scope.";
    let request = CompileRequest::create(words).with_authoring_policy(policy());
    let mut unresolved = reading();
    unresolved["preparation_requests"][0]["evidence"][0]["quote"] = json!(
        "Approval may be needed now or at every execution; I have not decided which, so do not choose that scope."
    );
    unresolved["coverage"][0]["unresolved"] = json!(true);
    let author = Author::new([Some(unresolved.clone()), Some(unresolved.clone())]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let held = verify_document(
        &request,
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&author),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(held.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 2);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(*room.0.lock().unwrap(), 0);
    let record = &held.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["reading"], unresolved);
    assert_eq!(record["failure"]["kind"], "unresolved");
    assert_eq!(record["repair_stop"]["reason"], "repeated_feedback");
}

#[tokio::test]
async fn repair_transport_failure_stops_without_decisions_or_trial() {
    let author = Author::new([Some(mistaken()), Some(defect()), None]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let held = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&author),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(held.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 3);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(*room.0.lock().unwrap(), 0);
    let record = &held.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(record["attempts"][0]["audit"], defect());
    assert_eq!(record["failure"]["kind"], "call");
    assert_eq!(record["failure"]["stage"], "interpret_request");
}

#[tokio::test]
async fn invented_audit_evidence_cannot_keep_the_reader_repairing() {
    let invalid = json!({"status":"defect","defects":[
        {"location":"r1","kind":"missing","evidence":"invented requirement one","note":"Invented critique one."},
        {"location":"r2","kind":"scope","evidence":"invented requirement two","note":"Invented critique two."}
    ]});
    let author = Author::new([Some(mistaken()), Some(invalid.clone())]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let held = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&author),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(held.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 2);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(*room.0.lock().unwrap(), 0);
    let record = &held.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["audit"], invalid);
    assert_eq!(record["failure"]["stage"], "audit_interpretation");
    assert_eq!(record["failure"]["kind"], "evidence");
    assert_eq!(record["failure"]["path"], "audit.defects[0].evidence");
    assert_eq!(record["attempts"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn duplicating_the_same_audit_defect_is_not_repair_progress() {
    let first = defect();
    let mut repeated = first.clone();
    repeated["defects"] = json!([first["defects"][0], first["defects"][0]]);
    let author = Author::new([
        Some(mistaken()),
        Some(first),
        Some(mistaken()),
        Some(repeated),
    ]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let held = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&author),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(held.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 4);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(*room.0.lock().unwrap(), 0);
    let record = &held.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["repair_stop"]["reason"], "repeated_feedback");
    assert_eq!(record["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(
        record["attempts"][0]["audit"]["defects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        record["attempts"][1]["audit"]["defects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
