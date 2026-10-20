// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A schedule rides beside the candidate, never in its bytes (#1720). An obligation only its
//! words evidence is carried there with no call; one with evidence beyond them is still asked;
//! with no schedule beside the candidate every obligation is asked as before, and one only whole
//! schedule words evidence, judged missing, is told where such words ride.

use super::*;
use crate::CompileOutcome;
use crate::cognition::verify_document_presented;

/// The request: when, then the work the bytes carry.
const SCHEDULED: &str = "Tous les matins à 9h, write hello to ./out/result.txt.";
/// The words of `SCHEDULED` that say when the greeting is written.
const WHEN: &str = "Tous les matins à 9h";
/// The request read as a mixed obligation: the work and when it runs, in one quote.
const MIXED: &str = "Tous les matins à 9h, write hello";
/// The obligation that says when the workflow runs.
const RUNS: &str = "Run the workflow every morning at 9:00.";

fn scheduled_request() -> CompileRequest {
    CompileRequest::create(SCHEDULED).with_authoring_policy(policy())
}

/// The audited reading of `SCHEDULED`: the write the bytes carry, and the obligation `RUNS`
/// evidenced by `quotes`.
fn timed(quotes: &[&str]) -> Value {
    let evidence: Vec<Value> = (quotes.iter())
        .map(|quote| json!({"span": "request", "quote": quote}))
        .collect();
    json!({"runtime_obligations":[
            {"id":"r1","requirement":REQUIREMENT,"answer_keys":[],
             "evidence":[{"span":"request","quote":"write hello to ./out/result.txt"}]},
            {"id":"r2","requirement":RUNS,"evidence":evidence,"answer_keys":[]}],
        "preparation_requests":[],
        "coverage":[{"span":"request","runtime_ids":["r1","r2"],"preparation_ids":[],
            "unresolved":false}],
        "answer_coverage":[]})
}

/// A proposal with no pending choice, carrying `words` as its schedule when given.
fn presented(words: Option<&str>) -> Value {
    let mut presentation = json!({"kind":"proposal", "pending_choices":[], "selected":null});
    if let Some(words) = words {
        presentation["schedule"] = json!({"message":"u1", "excerpt":words, "zone":"Europe/Paris"});
    }
    presentation
}

/// The document verified on `reading`, presented as `presentation`, before the seat.
async fn verified(reading: Value, presentation: &Value, seat: &Seat) -> (CompileOutcome, usize) {
    let provider = Author::new([Some(reading), Some(audit())]);
    let out = verify_document_presented(
        &scheduled_request(),
        SOURCE,
        (&[], &[]),
        presentation,
        Cognition {
            provider: Some(&provider),
            seat: Some(seat),
        },
        None,
    )
    .await
    .unwrap();
    (out, provider.calls())
}

/// The question ids the seat was asked, in order.
fn asked(seat: &Seat) -> Vec<String> {
    (seat.asked.lock().unwrap().iter())
        .map(|question| question.id.clone())
        .collect()
}

/// The first verification attempt recorded on `out`.
fn attempt(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["semantic_verification"][0].clone()
}

/// The engine fact of `attempt` whose `key` is `value`.
fn engine<'a>(attempt: &'a Value, key: &str, value: &str) -> Option<&'a Value> {
    (attempt["engine"].as_array().into_iter().flatten()).find(|fact| fact[key] == value)
}

#[tokio::test]
async fn an_obligation_only_the_schedule_beside_evidences_is_settled_with_no_call() {
    let presentation = presented(Some(WHEN));
    let seat = Seat::new(false);
    let (out, calls) = verified(timed(&[WHEN]), &presentation, &seat).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(calls, 2, "the reading and its audit only");
    // The engine's facts settle the extra question too: one call in all.
    assert_eq!(asked(&seat), ["verify-part-0"]);
    let attempt = attempt(&out);
    assert_eq!(attempt["settled_by"], "verify-obligations");
    let settled = engine(&attempt, "question", "verify-part-1").expect("settled by the engine");
    assert_eq!(settled["settled"], "beside_candidate");
    assert_eq!(
        (&settled["by"], &settled["obligation"]),
        (&json!("engine"), &json!(RUNS))
    );
    assert_eq!(settled["schedule"], presentation["schedule"]);
    let claim = engine(&attempt, "claim", "audited program obligations").expect("the claim");
    assert_eq!(claim["program_obligations"], json!([REQUIREMENT]));
    assert_eq!(claim["beside_candidate"], json!([RUNS]));
    assert_eq!(claim["authority"], false);
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(decision["presentation"], presentation);
}

#[tokio::test]
async fn an_obligation_with_evidence_beyond_the_schedule_is_still_asked() {
    // « write hello » reaches outside the schedule's words: still a program obligation.
    for quotes in [&[MIXED][..], &[WHEN, "write hello to ./out/result.txt"]] {
        let seat = Seat::new(false);
        let (out, _) = verified(timed(quotes), &presented(Some(WHEN)), &seat).await;
        assert_eq!(out.status, CompileStatus::Ready, "{quotes:?}: {out:#?}");
        let ids = asked(&seat);
        assert!(
            ids.iter().any(|id| id == "verify-part-1"),
            "{quotes:?}: {ids:?}"
        );
        let attempt = attempt(&out);
        assert!(
            engine(&attempt, "settled", "beside_candidate").is_none(),
            "{attempt:#}"
        );
        let claim = engine(&attempt, "claim", "audited program obligations").expect("the claim");
        assert_eq!(claim["program_obligations"], json!([REQUIREMENT, RUNS]));
        assert!(claim.get("beside_candidate").is_none(), "{claim:#}");
    }
}

#[tokio::test]
async fn without_a_schedule_beside_the_candidate_every_obligation_is_judged() {
    let seat = Seat::new(false);
    let (out, _) = verified(timed(&[WHEN]), &presented(None), &seat).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let ids = asked(&seat);
    assert_eq!(ids, ["verify-part-0", "verify-part-1"]);
    let judged = attempt(&out);
    assert!(engine(&judged, "settled", "beside_candidate").is_none());
    // Judged missing, the obligation only schedule words evidence is told where they ride.
    let seat = Seat::new(true);
    let (out, _) = verified(timed(&[WHEN]), &presented(None), &seat).await;
    assert_ne!(out.status, CompileStatus::Ready);
    let notes = attempt(&out)["notes"].clone();
    let note = |defect: &str| {
        (notes.as_array().into_iter().flatten())
            .find(|entry| entry["defect"] == defect)
            .map(|entry| entry["note"].clone())
    };
    let hint = note(RUNS).expect("the schedule's defect");
    assert!(
        hint.as_str()
            .is_some_and(|said| said.contains("`propose.schedule`"))
    );
    assert_eq!(
        note(REQUIREMENT),
        Some(json!("the judge finds no task performing it"))
    );
    let said = (out.diagnostics.iter())
        .filter(|diagnostic| diagnostic.target == "semantic_verification")
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();
    assert!(
        said.iter()
            .any(|said| said.contains(RUNS) && said.contains("propose.schedule"))
    );
}

#[tokio::test]
async fn a_schedule_presented_in_another_shape_is_refused_before_any_call() {
    let mut extra = presented(Some(WHEN));
    extra["schedule"]["cron"] = json!("0 9 * * *");
    let mut blank = presented(Some(WHEN));
    blank["schedule"]["excerpt"] = json!("  ");
    let mut unnamed = presented(Some(WHEN));
    unnamed["schedule"]
        .as_object_mut()
        .unwrap()
        .remove("message");
    for presentation in [extra, blank, unnamed] {
        let seat = Seat::new(false);
        let (out, calls) = verified(timed(&[WHEN]), &presentation, &seat).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{presentation:#}");
        assert_eq!(calls, 0, "{presentation:#}");
        assert!(asked(&seat).is_empty(), "{presentation:#}");
        let refused =
            (out.diagnostics.iter()).any(|diagnostic| diagnostic.target == "presentation");
        assert!(refused, "{presentation:#}");
    }
}
