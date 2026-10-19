// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[tokio::test]
async fn unresolved_coverage_names_its_location_without_claiming_a_judge_abstained() {
    let mut unresolved = reading();
    unresolved["coverage"][0]["unresolved"] = json!(true);
    let author = Author::new([Some(unresolved.clone()), Some(unresolved)]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let out = verify_document(
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
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(
        author.calls(),
        2,
        "unresolved meaning repeats without an audit or judgment"
    );
    assert_eq!(*room.0.lock().unwrap(), 0);
    assert!(seat.asked.lock().unwrap().is_empty());
    let record = &out.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["failure"]["kind"], "unresolved");
    assert_eq!(record["failure"]["path"], "coverage[0].unresolved");
    assert_eq!(record["valid"], false);
    let findings: Vec<_> = out
        .diagnostics
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert!(
        findings
            .iter()
            .any(|text| text.contains("interpret_request.coverage[0].unresolved: unresolved"))
    );
    assert!(
        !findings
            .iter()
            .any(|text| text.contains("judge could not settle") || text.contains("abstained"))
    );
    let attempt = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    assert_eq!(attempt["declined"], false);
    assert_eq!(attempt["settled"], false);
    assert_eq!(attempt["attempted"], 0);
}

#[tokio::test]
async fn malformed_reading_json_keeps_position_without_echoing_the_raw_response() {
    let author = Author::new([]);
    author
        .replies
        .lock()
        .unwrap()
        .push_back(Some("{not_json_sensitive_fixture".into()));
    let seat = Seat::new(false);
    let out = verify_document(
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
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 1);
    assert!(seat.asked.lock().unwrap().is_empty());
    let record = &out.provenance.decision.as_ref().unwrap()["request_interpretation"];
    assert_eq!(record["failure"]["kind"], "invalid_json");
    assert_eq!(record["failure"]["line"], 1);
    assert_eq!(record["failure"]["column"], 2);
    assert_eq!(record["failure"]["category"], "Syntax");
    assert!(!record.to_string().contains("not_json_sensitive_fixture"));
    assert!(
        !out.diagnostics
            .iter()
            .any(|finding| finding.message.contains("not_json_sensitive_fixture"))
    );
}

#[tokio::test]
async fn a_fabricated_evidence_quote_is_distinct_from_an_incomplete_call() {
    let mut invented = reading();
    invented["runtime_obligations"][0]["evidence"][0]["quote"] = json!("publish private data");
    let author = Author::new([Some(invented.clone()), Some(invented)]);
    let seat = Seat::new(false);
    let out = verify_document(
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
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 2);
    assert!(seat.asked.lock().unwrap().is_empty());
    let failure = &out.provenance.decision.as_ref().unwrap()["request_interpretation"]["failure"];
    assert_eq!(failure["kind"], "evidence");
    assert_eq!(failure["path"], "runtime_obligations[0].evidence[0].quote");
    assert!(
        out.diagnostics
            .iter()
            .any(|finding| finding.message.contains("not an exact substring"))
    );
}

#[tokio::test]
async fn an_audit_defect_reaches_the_tool_finding_with_its_evidence_and_note() {
    let audit = json!({"status":"defect","defects":[{
        "location":"p1", "kind":"scope", "evidence":"Ask me now which greeting to use.",
        "note":"The interpretation put a conversation question in runtime."}]});
    let author = Author::new([
        Some(reading()),
        Some(audit.clone()),
        Some(reading()),
        Some(audit.clone()),
    ]);
    let seat = Seat::new(false);
    let out = verify_document(
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
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(author.calls(), 4);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["request_interpretation"]["audit"],
        audit
    );
    let message = &out
        .diagnostics
        .iter()
        .find(|finding| finding.target == "request_interpretation")
        .unwrap()
        .message;
    assert!(message.contains("p1") && message.contains("scope"));
    assert!(message.contains("Ask me now which greeting to use."));
    assert!(message.contains("The interpretation put a conversation question in runtime."));
    assert!(message.contains("this is not a workflow defect"));
    assert!(
        !out.diagnostics
            .iter()
            .any(|finding| finding.message.contains("judge could not settle"))
    );
}

/// A reading that covers one span in two rows, or repeats a reference, is said once before it is
/// judged: the same claims, no repair round for a repetition (live gauntlet wave 6 r11,
/// 2026-10-18: « coverage[1].span · This identifier is repeated » started 2.7, then 9.7 minutes
/// of repair on a slow model).
#[test]
fn a_repeated_coverage_row_is_said_once_before_it_is_judged() {
    let mut repeated = reading();
    let split = json!({"span":"request","runtime_ids":["r1","r1"],"preparation_ids":[],
        "unresolved":false});
    repeated["coverage"][0]["runtime_ids"] = json!([]);
    repeated["coverage"].as_array_mut().unwrap().push(split);
    super::super::diagnostics::merged(&mut repeated);
    assert_eq!(repeated, reading(), "the same claims, said once");
    // A row it cannot read as a coverage row is left for the validation to refuse.
    let mut odd = reading();
    odd["coverage"]
        .as_array_mut()
        .unwrap()
        .push(json!({"span":"request","note":"x"}));
    super::super::diagnostics::merged(&mut odd);
    assert_eq!(odd["coverage"].as_array().unwrap().len(), 2);
    let source = input(INTENT, &request());
    assert!(super::super::diagnostics::validate(&source, &odd).is_err());
}
