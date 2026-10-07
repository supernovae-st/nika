// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rejections a host carries from an earlier round of the same conversation (R6,
//! `CompileRequest::with_declined`): bytes a judge answered and rejected there are never asked of
//! it again. The attempt repeats that verdict with no call, recorded as carried and apart from a
//! repeat of this compile's own attempt, and the route says so. An abstention is never carried (a
//! new round may decide what that round left held), and neither is another judge's rejection
//! nor a rejection of other bytes.

use std::time::Duration;

use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::{Value, json};

use super::super::{Declined, Judge, judged_native, knowledge, native_verdict, verdict_on};
use super::attempts::{
    INTENT, Judged, MODEL, PART, attempts, declined, doubting, journaled, judged, ready,
};
use super::{Approving, OMITTED, route};
use crate::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus};

/// The route a verdict carried from an earlier round takes.
const CARRIED: &str = "verify: same bytes, rejected in an earlier round";

/// The provider judge seated as [`MODEL`].
fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2))
}

/// The attempt an earlier round recorded when the doubting judge rejected the greeting's bytes,
/// its one part found missing, no task performing it: what a host carries.
async fn rejected_earlier() -> Value {
    let (earlier, _) = declined(judged(&doubting(), &policy(), ready(), 0).await);
    let attempt = attempts(&earlier)[0].clone();
    let flags = ["declined", "rejected", "settled", "carried"].map(|key| attempt[key].clone());
    assert_eq!(
        flags,
        [json!(true), json!(true), json!(false), json!(false)]
    );
    assert_eq!(attempt["defects"], json!([PART]));
    attempt
}

/// The native verdict of `out` under `request`, by `provider` seated as [`MODEL`].
async fn judged_under<P: ProviderInferDyn>(
    provider: &P,
    request: &CompileRequest,
    out: CompileOutcome,
) -> Judged {
    let reading = crate::lexicon::read(INTENT);
    let seats = (provider, None);
    native_verdict(INTENT, &reading, &policy(), seats, request, out, 0, None).await
}

/// Bytes a judge rejected in an earlier round, carried by the host, are never asked of it again
/// (R6): the native verdict makes no call and repeats that verdict (its defects and notes, its
/// doubt, the rejection), recording an attempt that asked nothing, says it was carried and names
/// no attempt of this compile; the route says so. The door withdraws the candidate, never READY
/// on a second vote of the same judge.
#[tokio::test]
async fn a_rejection_carried_from_an_earlier_round_is_never_asked_again() {
    let earlier = rejected_earlier().await;
    let judge = doubting();
    let request = CompileRequest::create(INTENT).with_declined(vec![earlier.clone()]);
    let fresh = ready();
    let mut routed = route(&fresh);
    let (out, verdict) = declined(judged_under(&judge, &request, fresh).await);
    assert!(judge.told.lock().unwrap().is_empty(), "no call");
    assert_eq!(journaled(&out), 0);
    assert!(verdict.carried);
    assert_eq!(verdict.same_bytes_as, None);
    assert_eq!(verdict.defects, [PART]);
    assert_eq!(verdict.notes, [(PART.to_owned(), OMITTED.to_owned())]);
    assert_eq!(verdict.doubt, ["unfaithful"]);
    assert_eq!(verdict.declined, Declined::Rejected);
    assert!(verdict.records.is_empty() && verdict.judgments.is_empty());
    assert_eq!(
        (verdict.attempted, verdict.returned, verdict.consumed),
        (0, 0, 0)
    );
    let sha = knowledge::sha256(out.candidate.as_deref().unwrap());
    assert_eq!(earlier["candidate_sha256"], json!(sha));
    let expected = json!({
        "attempt": 0,
        "judge": {"seat": MODEL, "kind": "authoring_provider"},
        "attempted": 0, "returned": 0, "consumed": 0,
        "usage": {"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true},
        "reference": earlier["reference"],
        "questions": [],
        "defects": [PART], "unknown": [], "doubt": ["unfaithful"], "contested": [],
        "unsettled": [],
        "notes": [{"defect": PART, "note": OMITTED}],
        "settled_by": null,
        "candidate_sha256": sha,
        "declined": true, "rejected": true, "settled": false, "stopped": false,
        "whole_asked": true,
        "request": INTENT,
        "same_bytes_as": null,
        "carried": true,
        "context_sha256": earlier["context_sha256"],
        "read_back": 0,
    });
    assert_eq!(attempts(&out), [expected]);
    routed.push(CARRIED.to_owned());
    assert_eq!(route(&out), routed);
    // The native door withdraws it: the defect stands, nothing is READY.
    let reading = crate::lexicon::read(INTENT);
    let seats = (&judge, None);
    let door = judged_native(INTENT, &reading, &policy(), seats, &request, ready()).await;
    assert!(judge.told.lock().unwrap().is_empty(), "no call");
    assert_eq!(door.status, CompileStatus::Incomplete, "{door:#?}");
    assert!(door.candidate.is_none() && door.provenance.plan.is_none());
    let steps = route(&door);
    assert_eq!(steps[steps.len() - 2..], [CARRIED, "verify: not ready"]);
}

/// Only a rejection of these very bytes by this very judge, against this very request and
/// context, is carried (R6): an earlier round's abstention left its bytes held for a new round to
/// decide, another judge's rejection (another seat, or the same name in another kind) binds only
/// that judge, a rejection of other bytes binds none of these, a rejection of another request (a
/// corrected one) or of no recorded request binds nothing this request asks, and one judged
/// beside other answers or another observed world binds nothing judged beside these. Each time the judge is asked
/// once and its faithful answer carries the request, recorded as no carried verdict.
#[tokio::test]
async fn only_this_judges_rejection_of_these_bytes_against_this_request_is_carried() {
    let earlier = rejected_earlier().await;
    assert_eq!(earlier["request"], INTENT);
    let altered = |key: &str, value: Value| {
        let mut attempt = earlier.clone();
        attempt[key] = value;
        attempt
    };
    let mut seat = earlier["judge"].clone();
    seat["seat"] = json!("mock/other-judge");
    let mut kind = earlier["judge"].clone();
    kind["kind"] = json!("decision_seat");
    let other = json!(knowledge::sha256("nika: another-workflow\n"));
    let corrected = json!("Write the text hello to ./out/greeting.txt.");
    let mut unrecorded = earlier.clone();
    unrecorded.as_object_mut().unwrap().remove("request");
    let cases = [
        ("abstained", altered("rejected", json!(false))),
        ("another seat", altered("judge", seat)),
        ("another kind", altered("judge", kind)),
        ("other bytes", altered("candidate_sha256", other)),
        ("another request", altered("request", corrected)),
        ("no recorded request", altered("request", Value::Null)),
        ("no request field", unrecorded),
        (
            "another context",
            altered("context_sha256", json!("0".repeat(64))),
        ),
    ];
    for (case, attempt) in cases {
        let approving = Approving::default();
        let request = CompileRequest::create(INTENT).with_declined(vec![attempt]);
        let Ok(out) = judged_under(&approving, &request, ready()).await else {
            panic!("{case}: the judge is asked and carries the request");
        };
        assert_eq!(out.status, CompileStatus::Ready, "{case}");
        assert_eq!(approving.told.lock().unwrap().len(), 1, "{case}");
        let recorded = attempts(&out);
        let how = ["settled_by", "carried", "same_bytes_as", "attempted"]
            .map(|key| recorded[0][key].clone());
        let decided = [json!("verify-request"), json!(false), Value::Null, json!(1)];
        assert_eq!(how, decided, "{case}");
        assert!(!route(&out).iter().any(|step| step == CARRIED), "{case}");
    }
}

/// The verdict a COLD, WARM or replayed round asks repeats a carried rejection too (R6): neither
/// the pending clause nor the whole request is asked, the earlier verdict is returned with no
/// call, carried, its usage nothing, and the route says so.
#[tokio::test]
async fn a_carried_rejection_asks_no_question_of_the_remainder() {
    let earlier = rejected_earlier().await;
    let clause = "Write the text hello";
    let mut settled = ready();
    settled.provenance.decision = Some(json!({"pending": {"open": [
        {"clause": clause, "witness": null, "spans": [[0, clause.len()]]},
        {"clause": INTENT, "witness": null, "spans": [[0, INTENT.len()]]},
    ]}}));
    let request = CompileRequest::create(INTENT).with_declined(vec![earlier.clone()]);
    let approving = Approving::default();
    let policy = policy();
    let judge = Judge::Provider(&policy, &approving);
    let plan = crate::plan::Plan::default();
    let mut out = crate::initial();
    let verdict = verdict_on((INTENT, &request, &plan), &settled, &judge, None, &mut out).await;
    assert!(approving.told.lock().unwrap().is_empty(), "no call");
    assert!(verdict.carried && verdict.same_bytes_as.is_none());
    assert_eq!(verdict.defects, [PART]);
    assert_eq!(verdict.notes, [(PART.to_owned(), OMITTED.to_owned())]);
    assert_eq!(verdict.declined, Declined::Rejected);
    let sha = earlier["candidate_sha256"].as_str();
    assert_eq!(verdict.candidate_sha256.as_deref(), sha);
    assert!(verdict.records.is_empty() && verdict.judgments.is_empty());
    let nothing = json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true});
    assert_eq!(verdict.usage, nothing);
    assert_eq!(route(&out), [CARRIED]);
}
