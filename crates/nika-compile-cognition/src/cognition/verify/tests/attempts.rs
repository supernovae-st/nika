// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The attempts of one compile (R4 A11, R6): bytes a judge answered and did not accept are never
//! asked of it again, the attempt repeating its earlier verdict with no call, while a judge that
//! answered nothing, or another judge, is asked; each attempt's record states how the judge
//! declined and what it asked, its findings once each; its usage counts every physical request;
//! and the forensic summary states the last verdict and whether it judged the final bytes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};

use super::super::{Declined, Judge, Verdict, knowledge, native_verdict, record, tidy};
use super::{Approving, Envelope, GREETING, OMITTED, Seat, route};
use crate::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus};

pub(super) const INTENT: &str = "Write the text hello to ./out/result.txt.";
/// The one part of [`INTENT`], as a doubted whole request asks it alone.
pub(super) const PART: &str = "Write the text hello to ./out/result.txt";
pub(super) const MODEL: &str = "mock/judge";

/// A native verdict's consequence: the outcome as judged, or the outcome and the verdict that
/// leave it not READY.
pub(super) type Judged = Result<CompileOutcome, Box<(CompileOutcome, Verdict)>>;

/// The greeting's candidate, READY before its judgment.
pub(super) fn ready() -> CompileOutcome {
    let mut ready = crate::initial();
    nika_compile::surface::finish(GREETING.to_owned(), &mut ready);
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    ready
}

/// The native verdict of `out` as attempt `attempt`, by `provider` seated as `policy`'s model.
pub(super) async fn judged<P: ProviderInferDyn>(
    provider: &P,
    policy: &AuthoringPolicy,
    out: CompileOutcome,
    attempt: usize,
) -> Judged {
    let request = CompileRequest::create(INTENT);
    let reading = crate::lexicon::read(INTENT);
    let seats = (provider, None);
    native_verdict(
        INTENT, &reading, policy, seats, &request, out, attempt, None,
    )
    .await
}

/// The not-READY half of a native verdict.
pub(super) fn declined(judged: Judged) -> (CompileOutcome, Verdict) {
    let Err(judged) = judged else {
        panic!("these bytes were not accepted");
    };
    *judged
}

/// The verification attempts an outcome records, in order.
pub(super) fn attempts(out: &CompileOutcome) -> Vec<Value> {
    (out.provenance.decision.as_ref())
        .and_then(|decision| decision["semantic_verification"].as_array().cloned())
        .unwrap_or_default()
}

/// The authoring journal's entries so far.
pub(super) fn journaled(out: &CompileOutcome) -> usize {
    (out.provenance.authoring.as_ref()).map_or(0, |receipt| receipt.context.len())
}

/// A judge doubting the greeting and finding its one part missing, no task performing it.
pub(super) fn doubting() -> Approving {
    Approving {
        doubt: true,
        missing: vec![0],
        ..Approving::default()
    }
}

/// Bytes a judge answered and did not accept are never asked of it again (R6): the next attempt
/// on the same bytes makes no call and repeats the earlier verdict (its defects and notes, its
/// doubt, how it declined), recording an attempt that asked nothing, names the attempt it
/// repeats and stays not READY.
#[tokio::test]
async fn bytes_a_judge_declined_are_never_asked_of_it_again() {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let judge = doubting();
    let (first, verdict) = declined(judged(&judge, &policy, ready(), 0).await);
    assert_eq!(verdict.defects, [PART]);
    let asked = judge.told.lock().unwrap().len();
    assert_eq!(asked, 3, "the request, its part, the task question");
    let sent = journaled(&first);
    let (again, repeated) = declined(judged(&judge, &policy, first, 1).await);
    assert_eq!(judge.told.lock().unwrap().len(), asked, "no call");
    assert_eq!(journaled(&again), sent);
    assert_eq!(repeated.defects, [PART]);
    assert_eq!(repeated.notes, [(PART.to_owned(), OMITTED.to_owned())]);
    assert_eq!(repeated.doubt, ["unfaithful"]);
    assert_eq!(repeated.declined, Declined::Rejected);
    assert_eq!(repeated.same_bytes_as, Some(0));
    assert!(repeated.records.is_empty() && repeated.judgments.is_empty());
    assert_eq!(
        again.status,
        CompileStatus::Ready,
        "returned as judged, never offered"
    );
    let recorded = attempts(&again);
    let sha = knowledge::sha256(again.candidate.as_deref().unwrap());
    let expected = json!({
        "attempt": 1,
        "judge": {"seat": MODEL, "kind": "authoring_provider"},
        "attempted": 0, "returned": 0, "consumed": 0,
        "usage": {"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true},
        "reference": recorded[0]["reference"],
        "questions": [],
        "defects": [PART], "unknown": [], "doubt": ["unfaithful"], "contested": [],
        "unsettled": [],
        "notes": [{"defect": PART, "note": OMITTED}],
        "settled_by": null,
        "candidate_sha256": sha,
        "declined": true, "rejected": true, "settled": false, "stopped": false,
        "whole_asked": true,
        "request": INTENT,
        "same_bytes_as": 0,
        "carried": false,
        "context_sha256": recorded[0]["context_sha256"],
        "read_back": 0,
    });
    assert_eq!(recorded.len(), 2);
    assert!(recorded[0]["context_sha256"].is_string());
    assert_eq!(recorded[1], expected);
    assert_eq!(recorded[0]["same_bytes_as"], Value::Null);
    assert_eq!(recorded[0]["attempted"], 3);
    let routed = route(&again);
    assert_eq!(
        routed.last().map(String::as_str),
        Some("verify: same bytes, earlier verdict stands")
    );
}

/// A judge that answered nothing (its call refused) declined nothing: the same judge is asked
/// again on the same bytes and its answer decides them. Bytes one judge declined are asked of
/// another judge, under its own name.
#[tokio::test]
async fn a_judge_that_answered_nothing_and_another_judge_are_asked_again() {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let authority = Arc::new(Envelope::new(0, "authorize judge calls"));
    let refusing = Seat::new(Approving::default(), authority.clone());
    let (unanswered, verdict) = declined(judged(&refusing, &policy, ready(), 0).await);
    assert_eq!(authority.account(), json!({"sent": 0, "refused": 1}));
    assert_eq!(verdict.unknown, [INTENT]);
    assert!(verdict.stopped && verdict.declined == Declined::No);
    assert_eq!(attempts(&unanswered)[0]["declined"], false);
    let approving = Approving::default();
    let Ok(decided) = judged(&approving, &policy, unanswered, 1).await else {
        panic!("the same judge, asked again, carries the request");
    };
    assert_eq!(approving.told.lock().unwrap().len(), 1);
    let recorded = attempts(&decided);
    assert_eq!(recorded[1]["settled_by"], "verify-request");
    assert_eq!(recorded[1]["same_bytes_as"], Value::Null);
    let (doubted, _) = declined(judged(&doubting(), &policy, ready(), 0).await);
    let other = AuthoringPolicy::new("mock/other-judge", 256, Duration::from_secs(2));
    let approving = Approving::default();
    let Ok(decided) = judged(&approving, &other, doubted, 1).await else {
        panic!("another judge is asked on bytes the first one declined");
    };
    assert_eq!(approving.told.lock().unwrap().len(), 1);
    let recorded = attempts(&decided);
    let judge = json!({"seat": "mock/other-judge", "kind": "authoring_provider"});
    assert_eq!(
        (&recorded[1]["judge"], &recorded[1]["settled_by"]),
        (&judge, &json!("verify-request"))
    );
    assert_eq!(recorded[1]["same_bytes_as"], Value::Null);
}

/// Each finding once, at its first place: a defect, an unknown, a contested part and a reason
/// named twice are kept once, and a defect's note is the first one given.
#[test]
fn each_finding_and_note_is_kept_once_at_its_first_place() {
    let owned = |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
    let mut verdict = Verdict {
        defects: owned(&["a", "b", "a"]),
        unknown: owned(&["x", "x", "y"]),
        contested: owned(&["c", "c"]),
        unsettled: owned(&["r", "r"]),
        notes: vec![
            ("a".to_owned(), "first".to_owned()),
            ("a".to_owned(), "second".to_owned()),
            ("b".to_owned(), "third".to_owned()),
        ],
        ..Verdict::default()
    };
    tidy(&mut verdict);
    assert_eq!(verdict.defects, ["a", "b"]);
    assert_eq!(verdict.unknown, ["x", "y"]);
    assert_eq!(verdict.contested, ["c"]);
    assert_eq!(verdict.unsettled, ["r"]);
    let notes = [("a", "first"), ("b", "third")].map(|(d, n)| (d.to_owned(), n.to_owned()));
    assert_eq!(verdict.notes, notes);
}

/// Each attempt's record states the judge, its counts, what it found and how it declined: a
/// rejection, whether a call got no answer and stopped it, whether the whole request was asked
/// and which, and the earlier attempt it repeats; a second attempt is appended after the first.
#[test]
fn the_record_states_how_the_judge_declined_and_what_it_asked() {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Approving::default();
    let judge = Judge::Provider(&policy, &provider);
    let verdict = Verdict {
        defects: vec!["d".to_owned()],
        unknown: vec!["u".to_owned()],
        doubt: vec!["unfaithful".to_owned()],
        contested: vec!["c".to_owned()],
        unsettled: vec!["why".to_owned()],
        notes: vec![(
            "d".to_owned(),
            "the judge points to the task save".to_owned(),
        )],
        attempted: 4,
        returned: 3,
        consumed: 2,
        candidate_sha256: Some("abc".to_owned()),
        declined: Declined::Rejected,
        stopped: true,
        whole_asked: true,
        request: Some("the request".to_owned()),
        same_bytes_as: Some(1),
        ..Verdict::default()
    };
    let abstained = Verdict {
        unknown: vec!["the request".to_owned()],
        doubt: vec!["none".to_owned()],
        declined: Declined::Abstained,
        settled_by: Some("verify-observed"),
        ..Verdict::default()
    };
    let mut out = crate::initial();
    record(&mut out, &judge, &verdict, 2);
    record(&mut out, &judge, &abstained, 3);
    let recorded = attempts(&out);
    let first = json!({
        "attempt": 2,
        "judge": {"seat": MODEL, "kind": "authoring_provider"},
        "attempted": 4, "returned": 3, "consumed": 2,
        "usage": null, "reference": null, "questions": [],
        "defects": ["d"], "unknown": ["u"], "doubt": ["unfaithful"], "contested": ["c"],
        "unsettled": ["why"],
        "notes": [{"defect": "d", "note": "the judge points to the task save"}],
        "settled_by": null, "candidate_sha256": "abc",
        "declined": true, "rejected": true, "settled": false, "stopped": true,
        "whole_asked": true,
        "request": "the request", "same_bytes_as": 1, "carried": false,
        "context_sha256": null, "read_back": 0,
    });
    assert_eq!(recorded, [first, recorded[1].clone()]);
    let second = &recorded[1];
    let how = [
        "attempt",
        "declined",
        "rejected",
        "settled",
        "stopped",
        "whole_asked",
        "request",
        "carried",
    ]
    .map(|key| second[key].clone());
    let expected = [
        json!(3),
        json!(true),
        json!(false),
        json!(false),
        json!(false),
        json!(false),
        Value::Null,
        json!(false),
    ];
    assert_eq!(how, expected);
    assert_eq!(second["settled_by"], "verify-observed");
    assert_eq!(second["same_bytes_as"], Value::Null);
    // A rejection carried from an earlier round is recorded so, apart from a repeat of this
    // compile's own attempt.
    let carried = Verdict {
        carried: true,
        declined: Declined::Rejected,
        defects: vec!["d".to_owned()],
        ..Verdict::default()
    };
    record(&mut out, &judge, &carried, 4);
    let third = &attempts(&out)[2];
    let how = ["carried", "same_bytes_as", "rejected", "settled"].map(|key| third[key].clone());
    assert_eq!(how, [json!(true), Value::Null, json!(true), json!(false)]);
}

/// A provider judge whose answer is cut at its opening output limit: every first request ends
/// at the limit, the request widened to the ceiling answers `faithful`.
struct Cut {
    calls: AtomicUsize,
}

impl ProviderInferDyn for Cut {
    async fn infer(&self, _request: InferRequest) -> Result<InferResponse, ProviderError> {
        let at = self.calls.fetch_add(1, Ordering::SeqCst);
        let stop = if at == 0 {
            StopReason::MaxTokens
        } else {
            StopReason::EndTurn
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": "faithful"}).to_string(),
            }],
            TokenUsage::new(3, 2),
            stop,
        ))
    }
}

/// The usage of an attempt counts the provider's physical requests (R4 A11): one question whose
/// cut answer was widened once is two requests, each journaled and charged, while the attempt
/// asked one question.
#[tokio::test]
async fn a_widened_provider_call_counts_twice_in_the_usage() {
    let policy =
        AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2)).with_initial_max_tokens(64);
    let cut = Cut {
        calls: AtomicUsize::new(0),
    };
    let Ok(out) = judged(&cut, &policy, ready(), 0).await else {
        panic!("the widened answer carries the request");
    };
    assert_eq!(cut.calls.load(Ordering::SeqCst), 2);
    let attempt = &attempts(&out)[0];
    let counts = ["attempted", "returned", "consumed"].map(|key| attempt[key].clone());
    assert_eq!(counts, [json!(1), json!(1), json!(1)]);
    let usage = json!({"calls": 2, "input_tokens": 6, "output_tokens": 4, "complete": true});
    assert_eq!(attempt["usage"], usage);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let calls: Vec<(&Value, &Value)> = (receipt.context.iter())
        .map(|entry| (&entry["call"], &entry["widened_to"]))
        .collect();
    let widened = [
        (&json!("judge_request"), &json!(256)),
        (&json!("judge_request"), &Value::Null),
    ];
    assert_eq!(calls, widened);
}

/// A call the explicit authority refuses before any byte leaves is journaled, but it is no
/// request (R4 A11): the attempt's usage counts the two requests sent (the whole request, then
/// its part found missing) and never the refused task question, whose refusal stops the
/// localization; the usage stays complete, the refusal having used nothing.
#[tokio::test]
async fn a_refused_call_is_journaled_but_never_counted_as_a_request() {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let authority = Arc::new(Envelope::new(2, "authorize more judge calls"));
    let bounded = Seat::new(doubting(), authority.clone());
    let (out, verdict) = declined(judged(&bounded, &policy, ready(), 0).await);
    assert_eq!(authority.account(), json!({"sent": 2, "refused": 1}));
    assert_eq!(journaled(&out), 3);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let kinds: Vec<&Value> = (receipt.context.iter())
        .map(|entry| &entry["result"]["failure_kind"])
        .collect();
    assert_eq!(kinds[2], "admission_refused", "{kinds:?}");
    assert!(verdict.stopped && verdict.defects.is_empty());
    assert_eq!(verdict.unknown, [PART, INTENT]);
    let attempt = &attempts(&out)[0];
    let counts = ["attempted", "returned", "consumed"].map(|key| attempt[key].clone());
    assert_eq!(counts, [json!(3), json!(2), json!(2)]);
    let usage = json!({"calls": 2, "input_tokens": 2, "output_tokens": 2, "complete": true});
    assert_eq!(attempt["usage"], usage);
}

/// The semantic judge's line of the forensic summary: how many attempts, what the last one left
/// (defects, unknowns, contested entries), whether its judge declined the bytes and what settled
/// them, and whether it judged the final candidate's very bytes (`bound`), other bytes, or no
/// final candidate exists to bind.
#[test]
fn the_forensic_summary_states_the_last_verdict_and_its_binding() {
    let request = CompileRequest::create(INTENT);
    let summary = |candidate: bool, attempts: Value| -> Value {
        let mut out = ready();
        if !candidate {
            out.candidate = None;
        }
        out.provenance.decision = Some(json!({"semantic_verification": attempts}));
        crate::cognition::forensic::record(&request, true, &mut out);
        let decision = out.provenance.decision.unwrap();
        decision["forensic"]["evidence"]["semantic_judge"].clone()
    };
    let sha = knowledge::sha256(ready().candidate.as_deref().unwrap());
    let held = json!([
        {"candidate_sha256": "another", "defects": ["a"], "declined": true, "settled_by": null},
        {"candidate_sha256": sha, "defects": [], "unknown": ["u"], "contested": ["p", "r"],
            "declined": true, "settled_by": null},
    ]);
    let expected = json!({"state": "recorded", "attempts": 2, "last_defects": 0,
        "last_unknown": 1, "last_contested": 2, "last_declined": true,
        "last_settled_by": null, "candidate_binding": "bound"});
    assert_eq!(summary(true, held.clone()), expected);
    let mut withdrawn = expected.clone();
    withdrawn["candidate_binding"] = json!("NOT_CAPTURED");
    assert_eq!(summary(false, held), withdrawn);
    let settled = json!([{"candidate_sha256": "another", "defects": [], "unknown": [],
        "contested": [], "declined": false, "settled_by": "verify-request"}]);
    let elsewhere = json!({"state": "recorded", "attempts": 1, "last_defects": 0,
        "last_unknown": 0, "last_contested": 0, "last_declined": false,
        "last_settled_by": "verify-request", "candidate_binding": "other_bytes"});
    assert_eq!(summary(true, settled), elsewhere);
}
