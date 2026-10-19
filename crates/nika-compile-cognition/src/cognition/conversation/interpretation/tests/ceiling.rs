// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A Run ceiling rides beside the candidate, never in its bytes (QA gauntlet q09: « budget max
//! 0,10 $ par run » read as « each execution must cost at most $0.10 », judged missing, cost 11
//! author calls). An obligation only the words stating the ceiling the proposal binds evidence is
//! carried beside the candidate with no call, and so is one whose requirement states only that
//! ceiling, whatever its quote spans (live gauntlet wave 7 r09); a requirement beyond the
//! ceiling's words is still asked; with no ceiling bound beside the candidate it is asked as
//! before.

use super::*;
use crate::CompileOutcome;
use crate::cognition::verify_document_presented;

/// The request: the work the bytes carry, then the ceiling for each run.
const BUDGETED: &str = "Write hello to ./out/result.txt, budget max 0,10 $ par run.";
/// The words of `BUDGETED` that state the ceiling.
const CEILING: &str = "budget max 0,10 $ par run";
/// The obligation the ceiling's words read as.
const COSTS: &str = "Each execution must cost at most $0.10.";

fn budgeted_request() -> CompileRequest {
    CompileRequest::create(BUDGETED).with_authoring_policy(policy())
}

/// The audited reading of `BUDGETED`: the write the bytes carry, and the obligation `COSTS`
/// evidenced by `quotes`.
fn priced(quotes: &[&str]) -> Value {
    priced_as(COSTS, quotes)
}

/// The audited reading of `BUDGETED`: the write the bytes carry, and the obligation
/// `requirement` evidenced by `quotes`.
fn priced_as(requirement: &str, quotes: &[&str]) -> Value {
    let evidence: Vec<Value> = (quotes.iter())
        .map(|quote| json!({"span": "request", "quote": quote}))
        .collect();
    json!({"runtime_obligations":[
            {"id":"r1","requirement":REQUIREMENT,"answer_keys":[],
             "evidence":[{"span":"request","quote":"Write hello to ./out/result.txt"}]},
            {"id":"r2","requirement":requirement,"evidence":evidence,"answer_keys":[]}],
        "preparation_requests":[],
        "coverage":[{"span":"request","runtime_ids":["r1","r2"],"preparation_ids":[],
            "unresolved":false}],
        "answer_coverage":[]})
}

/// A proposal with no pending choice, binding the ceiling `words` state when given.
fn presented(words: Option<&str>) -> Value {
    let mut presentation = json!({"kind":"proposal", "pending_choices":[], "selected":null});
    if let Some(words) = words {
        presentation["money"] = json!({"excerpt": words, "usd": 0.1});
    }
    presentation
}

/// The document verified on `reading`, presented as `presentation`, before the seat.
async fn verified(reading: Value, presentation: &Value, seat: &Seat) -> (CompileOutcome, usize) {
    let provider = Author::new([Some(reading), Some(audit())]);
    let out = verify_document_presented(
        &budgeted_request(),
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

/// The engine fact of the first verification attempt on `out` whose `key` is `value`.
fn engine(out: &CompileOutcome, key: &str, value: &str) -> Option<Value> {
    let attempt = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    (attempt["engine"].as_array().into_iter().flatten())
        .find(|fact| fact[key] == value)
        .cloned()
}

#[tokio::test]
async fn an_obligation_only_the_bound_ceiling_evidences_is_settled_with_no_call() {
    let presentation = presented(Some(CEILING));
    let seat = Seat::new(false);
    let (out, calls) = verified(priced(&[CEILING]), &presentation, &seat).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(calls, 2, "the reading and its audit only");
    assert_eq!(asked(&seat), ["verify-part-0"]);
    let settled = engine(&out, "question", "verify-part-1").expect("settled by the engine");
    assert_eq!(settled["settled"], "beside_candidate");
    assert_eq!(settled["obligation"], COSTS);
    assert_eq!(settled["money"], presentation["money"]);
    let claim = engine(&out, "claim", "audited program obligations").expect("the claim");
    assert_eq!(claim["program_obligations"], json!([REQUIREMENT]));
    assert_eq!(claim["beside_candidate"], json!([COSTS]));
}

/// A requirement that states only the bound ceiling is carried beside the candidate, though its
/// quote spans the work's words too (live gauntlet wave 7 r09, 2026-10-19: the quote held the
/// model clause, and the first proposal was blocked twice for a ceiling no task can carry).
#[tokio::test]
async fn a_requirement_that_states_only_the_bound_ceiling_is_settled_with_no_call() {
    let presentation = presented(Some(CEILING));
    let seat = Seat::new(false);
    let quotes = ["Write hello to ./out/result.txt, budget max 0,10 $"];
    let (out, calls) = verified(priced(&quotes), &presentation, &seat).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(calls, 2, "the reading and its audit only");
    assert_eq!(asked(&seat), ["verify-part-0"]);
    let settled = engine(&out, "question", "verify-part-1").expect("settled by the engine");
    assert_eq!(settled["settled"], "beside_candidate");
    assert_eq!(settled["obligation"], COSTS);
}

#[tokio::test]
async fn an_obligation_beyond_the_ceilings_words_or_with_none_bound_is_asked() {
    // A requirement beyond the ceiling's words, or no ceiling bound: a program obligation, asked.
    let beyond = "Write hello to ./out/result.txt, and each execution must cost at most $0.10.";
    for (requirement, quotes, words) in [
        (
            beyond,
            &["Write hello to ./out/result.txt, budget max 0,10 $"][..],
            Some(CEILING),
        ),
        (COSTS, &[CEILING][..], None),
    ] {
        let seat = Seat::new(false);
        let (out, _) = verified(priced_as(requirement, quotes), &presented(words), &seat).await;
        assert_eq!(out.status, CompileStatus::Ready, "{quotes:?}: {out:#?}");
        assert!(
            asked(&seat).iter().any(|id| id == "verify-part-1"),
            "{quotes:?}"
        );
        assert!(engine(&out, "settled", "beside_candidate").is_none());
    }
}

#[tokio::test]
async fn a_ceiling_presented_in_another_shape_is_refused_before_any_call() {
    let mut extra = presented(Some(CEILING));
    extra["money"]["source"] = json!("explicit");
    let mut negative = presented(Some(CEILING));
    negative["money"]["usd"] = json!(-1.0);
    let mut blank = presented(Some(CEILING));
    blank["money"]["excerpt"] = json!(" ");
    for presentation in [extra, negative, blank] {
        let seat = Seat::new(false);
        let (out, calls) = verified(priced(&[CEILING]), &presentation, &seat).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{presentation:#}");
        assert_eq!(calls, 0, "{presentation:#}");
        assert!(asked(&seat).is_empty(), "{presentation:#}");
    }
}
