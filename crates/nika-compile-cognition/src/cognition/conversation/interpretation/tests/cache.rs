// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

fn rejected_audit() -> Value {
    json!({"status":"defect","defects":[{"location":"r2","kind":"scope",
        "evidence":"Ask me now which greeting to use.",
        "note":"The conversation question is not requested at each workflow execution."}]})
}

#[test]
fn a_later_rejected_or_unfinished_audit_prevents_fallback_to_an_older_reading() {
    let source = input(INTENT, &request());
    let accepted = json!({"input_sha256":key(&source, &policy()),"reading":reading(),
        "audit":audit(),"valid":true});
    for next in [
        rejected_audit(),
        json!({"status":"unknown","defects":[]}),
        Value::Null,
    ] {
        let mut latest = accepted.clone();
        latest["audit"] = next;
        let request = request().with_declined(vec![
            json!({"request_interpretation":accepted}),
            json!({"request_interpretation":latest}),
        ]);
        assert!(earlier(&source, &policy(), &request).is_none());
    }
}

#[tokio::test]
async fn an_audit_defect_retries_the_interpretation_for_the_same_words_and_candidate() {
    let mut mistaken = reading();
    mistaken["runtime_obligations"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "id":"r2","requirement":"Ask the user which greeting to use at every execution.",
        "evidence":[{"span":"request","quote":"Ask me now which greeting to use."}],
        "answer_keys":[]}));
    mistaken["preparation_requests"] = json!([]);
    mistaken["coverage"][0]["runtime_ids"] = json!(["r1", "r2"]);
    mistaken["coverage"][0]["preparation_ids"] = json!([]);
    assert!(valid(&input(INTENT, &request()), &mistaken));
    let first = Author::new([
        Some(mistaken.clone()),
        Some(rejected_audit()),
        Some(mistaken),
        Some(rejected_audit()),
    ]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let held = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&first),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(held.status, CompileStatus::Incomplete);
    assert_eq!(first.calls(), 4);
    assert_eq!(*room.0.lock().unwrap(), 0);
    assert!(seat.asked.lock().unwrap().is_empty());
    let rejected = held.provenance.decision.unwrap()["semantic_verification"][0].clone();
    assert_eq!(
        rejected["request_interpretation"]["audit"],
        rejected_audit()
    );
    assert_eq!(rejected["declined"], false);
    let retry = Author::new([Some(reading()), Some(audit())]);
    let ready = verify_document(
        &request().with_declined(vec![rejected]),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&retry),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    assert_eq!(retry.calls(), 2);
    let requests = retry.requests.lock().unwrap();
    let ContentBlock::Text { text } = &requests[0].messages[1].content[0] else {
        panic!()
    };
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap(),
        input(INTENT, &request())
    );
    let asked = seat.asked.lock().unwrap();
    let obligations = asked
        .iter()
        .filter(|q| q.id.starts_with("verify-part-"))
        .collect::<Vec<_>>();
    assert_eq!(obligations.len(), 1);
    assert_eq!(
        obligations[0].state["obligation"]["requirement"],
        REQUIREMENT
    );
}
