// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A revision whose revised bytes the judge rejected in an earlier round of the conversation
//! (R6): the host carries that verdict (`with_declined`), the round fills the very same bytes
//! again, and the judge is never asked on them; the attempt repeats the verdict with no call,
//! recorded as carried. Its located defect is what the fills reopen from (an authoring call,
//! never a judge call on those bytes) when the round count allows a refill; with none left the
//! revision is withdrawn, never READY.
use super::*;

/// The route a verdict carried from an earlier round takes.
const CARRIED: &str = "verify: same bytes, rejected in an earlier round";

/// The fills of [`fills`]`("shipped")` spelled otherwise: the same selection, other bytes.
fn refilled() -> Value {
    json!({"fills": [
        {"task": "keep", "field": "expression", "value": "fromjson | map(select(.total > 10 and .status == \"shipped\"))"},
        {"task": "count", "field": "expression", "value": "fromjson | length"},
        {"task": "approve", "field": "args.message", "value": "${{ const.approval_message }}"}],
        "notes": "refilled"})
}

/// The earlier round refuses the revised bytes (the replaced part missing, its task named) with
/// no round left to refill them. The next round carries that verdict and fills the same bytes:
/// no judge is asked on them. Allowed a refill, it refills from the carried defect, and the
/// refilled bytes are judged and READY; with no refill left, the revision is withdrawn naming
/// the defect, never READY.
#[tokio::test]
async fn a_rejection_carried_from_an_earlier_round_reopens_the_revision_from_its_defect() {
    let (_, bytes, record) = base().await;
    let answers = vec![revised(link(PAID, SHIPPED)), fills("shipped")];
    let refusing = Semantic::judging(answers.clone(), vec!["unfaithful"], usize::MAX);
    let earlier = compile_with_provider(&revise(&bytes, &record, 0), &refusing)
        .await
        .unwrap();
    assert_eq!(refusing.calls(), 2 + JUDGED, "{earlier:#?}");
    assert_ne!(earlier.status, CompileStatus::Ready, "{earlier:#?}");
    let decision = earlier.provenance.decision.as_ref().unwrap();
    let rejected = decision["semantic_verification"][0].clone();
    let flags = ["rejected", "settled", "carried"].map(|key| rejected[key].clone());
    assert_eq!(
        flags,
        [json!(true), json!(false), json!(false)],
        "{rejected:#}"
    );
    assert_eq!(rejected["defects"], json!([REPLACED]), "{rejected:#}");
    // No refill left: the carried verdict holds the same bytes, with no judge question.
    let seat = Semantic::judging(answers.clone(), Vec::new(), usize::MAX);
    let carrying = revise(&bytes, &record, 0).with_declined(vec![rejected.clone()]);
    let out = compile_with_provider(&carrying, &seat).await.unwrap();
    assert_eq!(seat.calls(), 2, "the links and one fill: {out:#?}");
    assert!(seat.states.lock().unwrap().is_empty(), "no judge question");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let decision = out.provenance.decision.as_ref().unwrap();
    let verified = decision["semantic_verification"].as_array().unwrap();
    assert_eq!(verified.len(), 1, "{decision:#}");
    let fields = [
        "attempted",
        "questions",
        "carried",
        "same_bytes_as",
        "defects",
        "notes",
        "judge",
        "candidate_sha256",
    ]
    .map(|key| verified[0][key].clone());
    let expected = [
        json!(0),
        json!([]),
        json!(true),
        Value::Null,
        json!([REPLACED]),
        json!([{"defect": REPLACED, "note": POINTED}]),
        rejected["judge"].clone(),
        rejected["candidate_sha256"].clone(),
    ];
    assert_eq!(fields, expected, "{decision:#}");
    assert!(
        decision["route"].to_string().contains(CARRIED),
        "{decision:#}"
    );
    let named = (out.diagnostics.iter()).any(|d| {
        d.target == "semantic_verification"
            && d.message.contains(REPLACED)
            && d.message.contains("0 repair(s)")
    });
    assert!(named, "{out:#?}");
    // A refill granted: the fills reopen from the carried defect, and only the refilled bytes
    // are judged.
    let mut refill = answers;
    refill.push(refilled());
    let seat = Semantic::judging(refill, Vec::new(), usize::MAX);
    let carrying = revise(&bytes, &record, 1).with_declined(vec![rejected.clone()]);
    let out = compile_with_provider(&carrying, &seat).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let roles = roles(&out);
    assert_eq!(roles[..3], ["revision", "fill", "fill"], "{roles:?}");
    assert_eq!(
        seat.states.lock().unwrap().len(),
        1,
        "one judgment, of the refill"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let verified = decision["semantic_verification"].as_array().unwrap();
    assert_eq!(verified.len(), 2, "{decision:#}");
    assert_eq!(verified[0]["carried"], json!(true), "{decision:#}");
    assert_eq!(verified[0]["attempted"], json!(0), "{decision:#}");
    assert_ne!(
        verified[1]["candidate_sha256"], rejected["candidate_sha256"],
        "other bytes: {decision:#}"
    );
    assert_eq!(
        verified[1]["settled_by"],
        json!("verify-request"),
        "{decision:#}"
    );
    assert!(
        decision["route"].to_string().contains(CARRIED),
        "{decision:#}"
    );
}
