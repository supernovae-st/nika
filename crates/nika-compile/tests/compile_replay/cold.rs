// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The answer rounds of a COLD record (R4 A11, C3): the recorded plan replays the same
//! candidate with no authoring call, and a model's plan is READY only once the round's own
//! judge carries the whole request over the replayed bytes. With no judge it stays INCOMPLETE,
//! the whole request pending on those bytes, whatever round wrote the record and whatever the
//! record carries: a record is the plan's identity, never a judgment.
use super::*;

/// The fields of a plan record: the plan and its strategy word, in their order.
const PLAN_FIELDS: [&str; 10] = [
    "bindings",
    "constraints",
    "effects",
    "obligations",
    "operations",
    "rules",
    "slots",
    "strategy",
    "trigger",
    "unknowns",
];

/// A plan record is the plan and its strategy word alone: no mark of a judgment made or owed.
fn assert_plan_alone(record: &Value) {
    let fields: Vec<&String> = (record.as_object())
        .map(|record| record.keys().collect())
        .unwrap_or_default();
    assert_eq!(fields, PLAN_FIELDS, "{record:#}");
}

#[tokio::test]
async fn a_recorded_cold_plan_replays_the_same_candidate_with_no_authoring_call() {
    let provider = Provider::new(&proposal());
    let first = compile_with_provider(
        &CompileRequest::create(COLD_INTENT).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(
        first.provenance.strategy,
        Some(Strategy::Cold),
        "{first:#?}"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let plan = first
        .provenance
        .plan
        .clone()
        .expect("a settled plan is recorded");
    assert_eq!(
        plan["strategy"], "cold",
        "the plan record names its strategy"
    );
    // The round asked before any candidate: it judged nothing, and its record is the plan alone.
    assert!(first.candidate.is_none(), "{first:#?}");
    assert_plan_alone(&plan);
    // The original answer round: the provider is called again and may drift.
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let original = compile_with_provider(
        &answered(CompileRequest::create(COLD_INTENT).with_authoring_policy(policy())),
        &common::Judged::approving(&provider),
    )
    .await
    .unwrap();
    assert_eq!(original.status, CompileStatus::Ready, "{original:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    // The replayed answer round: the recorded plan, the same answers, no authoring call. A
    // model's plan is READY only on a judgment of the whole request over the bytes this round
    // replays (C3): the round's own judge, its seat, is asked that one question.
    let seat = Seat(AtomicU32::new(0));
    let replayed = compile_with_cognition(
        &answered(
            CompileRequest::create(COLD_INTENT)
                .with_authoring_policy(policy())
                .with_plan(plan.clone()),
        ),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        2,
        "zero provider calls"
    );
    assert_eq!(
        seat.0.load(Ordering::SeqCst),
        1,
        "one judgment, by the seat"
    );
    assert_eq!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert_eq!(replayed.candidate, original.candidate);
    assert_eq!(replayed.provenance.strategy, Some(Strategy::Cold));
    assert_eq!(replayed.provenance.authoring, None);
    assert_eq!(
        replayed.provenance.cognition,
        AuthoringCognition::DeterministicOnly
    );
    assert_eq!(
        route(&replayed),
        json!(["replayed plan", "verify: judged (decision_seat)"])
    );
    let judged = &outcome_document(&replayed)["provenance"]["decision"]["semantic_verification"];
    let asked = &judged[0]["questions"];
    assert_eq!(asked.as_array().map(Vec::len), Some(1), "{judged:#}");
    let whole = (&asked[0]["question"], &asked[0]["choice"]);
    assert_eq!(whole, (&json!("verify-request"), &json!("faithful")));
    let by = (&judged[0]["judge"], &judged[0]["settled_by"]);
    let seated = json!({"seat": "double/seat", "kind": "decision_seat"});
    assert_eq!(by, (&seated, &json!("verify-request")), "{judged:#}");
    // The round that carried the whole request records the very plan it replayed: no judgment
    // rides the record, so the next round judges its own bytes again.
    assert_eq!(replayed.provenance.plan.as_ref(), Some(&plan));
    let document = outcome_document(&replayed);
    assert_eq!(document["compile_version"], 1);
    assert_eq!(document["provenance"]["cognition"], "deterministicOnly");
    assert_eq!(document["provenance"]["strategy"], "cold");
    assert_eq!(
        document["provenance"]["decision"]["intent_sha256"],
        intent_sha256(COLD_INTENT)
    );
    // The deterministic door replays the same plan without any cognition at all.
    let deterministic = compile(&answered(
        CompileRequest::create(COLD_INTENT).with_plan(plan.clone()),
    ))
    .unwrap();
    assert_eq!(deterministic.candidate, original.candidate);
    assert_eq!(route(&deterministic), json!(["replayed plan"]));
    // With no judge, the bytes no round judged whole stay pending (R4 A11, C3).
    common::assert_waits_for_its_judge(&deterministic, COLD_INTENT);
    // An unanswered replay asks exactly the questions the fresh compile asked.
    let unanswered = compile(&CompileRequest::create(COLD_INTENT).with_plan(plan)).unwrap();
    assert_eq!(keys(&unanswered), keys(&first));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

/// A COLD record replayed with no judge stays INCOMPLETE, its whole request pending on the
/// replayed bytes (C3), whatever door the host takes. A request with no authoring policy and no
/// decision seat (the CLI answer round without `--authoring-model`) goes to the deterministic
/// door, which replays a model's plan with the whole request pending (the P-COLD-NO-POLICY
/// witness, green since the core replay keeps it pending).
#[tokio::test]
async fn a_cold_record_replayed_with_no_judge_waits_for_its_whole_request() {
    let provider = Provider::new(&proposal());
    let request = CompileRequest::create(COLD_INTENT).with_authoring_policy(policy());
    let first = compile_with_provider(&request, &provider).await.unwrap();
    let plan = first
        .provenance
        .plan
        .clone()
        .expect("a settled plan is recorded");
    let replay = answered(CompileRequest::create(COLD_INTENT).with_plan(plan));
    let cognition = Cognition::<nika_compile_cognition::NoProvider> {
        provider: None,
        seat: None,
    };
    let out = compile_with_cognition(&replay, cognition).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    common::assert_waits_for_its_judge(&out, COLD_INTENT);
    // The core's own door is the same door.
    let core = compile(&replay).unwrap();
    assert_eq!(core.candidate, out.candidate, "{core:#?}");
    common::assert_waits_for_its_judge(&core, COLD_INTENT);
}

/// The record the first round writes for [`COLD_INTENT`], before any candidate, and the keys of
/// the questions it asks.
async fn first_record(provider: &Provider) -> (Value, Vec<String>) {
    let request = CompileRequest::create(COLD_INTENT).with_authoring_policy(policy());
    let first = compile_with_provider(&request, provider).await.unwrap();
    assert!(first.candidate.is_none(), "{first:#?}");
    let asked = keys(&first).into_iter().map(str::to_owned).collect();
    let record = first.provenance.plan.expect("a settled plan is recorded");
    (record, asked)
}

/// Review scenario A (R4 A11): the round with the authoring model answers one question, the
/// others still open, and writes its record again before any candidate exists (the CLI rewrites
/// the sidecar when the observed world moved). The next round answers the rest with no judge:
/// the first candidate of that model plan waits for a judgment of its whole request on those
/// bytes, never READY on a record no round judged, and the author is never called again.
#[tokio::test]
async fn a_cold_record_rewritten_before_any_candidate_waits_for_its_whole_request() {
    let provider = Provider::new(&proposal());
    let (recorded, asked) = first_record(&provider).await;
    let partial = CompileRequest::create(COLD_INTENT)
        .with_authoring_policy(policy())
        .with_plan(recorded.clone())
        .answer("model", r#""mock/echo""#);
    let second = compile_with_provider(&partial, &provider).await.unwrap();
    assert!(second.candidate.is_none(), "{second:#?}");
    let open: Vec<&str> = (asked.iter().map(String::as_str))
        .filter(|key| *key != "model")
        .collect();
    assert_eq!(keys(&second), open, "{second:#?}");
    let rewritten = second.provenance.plan.clone().expect("the round's record");
    assert_eq!(rewritten, recorded, "the plan's identity: {rewritten:#}");
    let third = compile(&answered(
        CompileRequest::create(COLD_INTENT).with_plan(rewritten),
    ))
    .unwrap();
    common::assert_waits_for_its_judge(&third, COLD_INTENT);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{third:#?}");
}

/// Review scenario B (R4 A11, R6): a round's judge carries the whole request READY and the round
/// keeps its record; the next round, with no judge, replays that record. Whatever it carries —
/// the very plan the judged round replayed, another answer that changes the bytes, or fields
/// claiming the judgment (a pending mark set false, the digest of the judged bytes, the judged
/// round's verdicts and judgments) — nothing in a record is a judgment: the replayed bytes wait
/// for a judgment of the whole request made in their own round, never READY.
#[tokio::test]
async fn a_cold_record_answered_with_no_judge_after_a_judged_round_waits_for_its_whole_request() {
    let provider = Provider::new(&proposal());
    let (recorded, _) = first_record(&provider).await;
    let judge = common::JudgedSeat::approving(&common::NoChoice);
    let judged = compile_with_cognition(
        &answered(
            CompileRequest::create(COLD_INTENT)
                .with_authoring_policy(policy())
                .with_plan(recorded.clone()),
        ),
        Cognition {
            provider: Some(&provider),
            seat: Some(&judge),
        },
    )
    .await
    .unwrap();
    assert_eq!(judged.status, CompileStatus::Ready, "{judged:#?}");
    assert_eq!(judge.judged.load(Ordering::SeqCst), 1, "{judged:#?}");
    let kept = judged.provenance.plan.clone().expect("the READY record");
    assert_eq!(kept, recorded, "no judgment rides the record: {kept:#}");
    let bytes = judged.candidate.clone().expect("the judged bytes");
    let replay = |record: &Value| {
        compile(&answered(
            CompileRequest::create(COLD_INTENT).with_plan(record.clone()),
        ))
        .unwrap()
    };
    // The same answers, the same bytes: judged in another round, so pending in this one.
    let same = replay(&kept);
    common::assert_waits_for_its_judge(&same, COLD_INTENT);
    assert_eq!(same.candidate.as_deref(), Some(bytes.as_str()));
    // Another answer, other bytes no judge ever saw.
    let other = compile(
        &answered(CompileRequest::create(COLD_INTENT).with_plan(kept.clone())).answer(
            "const.refund_endpoint",
            r#""https://other.example.invalid/refunds""#,
        ),
    )
    .unwrap();
    common::assert_waits_for_its_judge(&other, COLD_INTENT);
    assert_ne!(other.candidate, same.candidate, "{other:#?}");
    // A record claiming the judgment: data, never a judgment.
    let decision = judged.provenance.decision.clone().unwrap_or_default();
    let claims = [
        ("whole_pending", json!(false)),
        (
            "whole_carried_sha256",
            json!(nika_compile::surface::sha256(&bytes)),
        ),
        ("settled_by", json!("verify-request")),
        (
            "semantic_verification",
            decision["semantic_verification"].clone(),
        ),
        (
            "judgments",
            json!([{"clause": COLD_INTENT, "span": [0, COLD_INTENT.len()],
            "disposition": "carried", "seat": "test/no-choice", "question": "verify-request"}]),
        ),
    ];
    let mut claimed = kept.clone();
    for (key, value) in claims {
        claimed[key] = value;
    }
    let carried = replay(&claimed);
    common::assert_waits_for_its_judge(&carried, COLD_INTENT);
    assert_eq!(carried.candidate, same.candidate, "{carried:#?}");
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "no authoring call"
    );
    assert_eq!(
        judge.judged.load(Ordering::SeqCst),
        1,
        "no judge after the judged round"
    );
}

/// RED witness (P-NO-STRATEGY-WORD), kept executable: a COLD record whose strategy word is gone
/// (an earlier engine's record, or one a host rewrote) is still a model's plan no law of the
/// reader read whole: replayed with no judge, its bytes must wait for a judgment of the whole
/// request, never READY. Today the core replay (`doors::replay`) and the cognition replay
/// (`resolve_create`) keep the whole request pending only for the words `cold` and `warm`, so a
/// record with no word (or an unknown one) is READY on the laws alone. Resume when a record that
/// does not name the reader's own HOT plan replays with its whole request pending: this test
/// must pass unchanged.
#[tokio::test]
async fn a_cold_record_without_its_strategy_word_still_waits_for_its_whole_request() {
    let provider = Provider::new(&proposal());
    let (mut recorded, _) = first_record(&provider).await;
    recorded
        .as_object_mut()
        .map(|record| record.remove("strategy"));
    let out = compile(&answered(
        CompileRequest::create(COLD_INTENT).with_plan(recorded),
    ))
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    common::assert_waits_for_its_judge(&out, COLD_INTENT);
}
