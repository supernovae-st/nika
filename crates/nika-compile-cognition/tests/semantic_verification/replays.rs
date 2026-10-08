// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The answer rounds of a record (R4 A11, Q2, C3): what a replay asks its round's judge, and
//! what settles it. A record replays its same bytes with no plan, transform or repair call, and
//! nothing it or the request carries is a judgment. A model's plan (COLD) replays with its whole
//! request pending again on the replayed bytes: the round's judge is asked the remainder the
//! core names (the clause no law reads), then the whole request, and only a faithful whole
//! request is READY; with no judge the round stays INCOMPLETE naming them. A judge that rejects
//! the bytes leaves the record with that rejection inside it, so every later round replaying it
//! repeats the rejection with no call, however often it replays; an abstention drops the record;
//! a judge whose call got no answer declined nothing and keeps the record for a later round.
use super::*;
use nika_compile_cognition::{Cognition, NoProvider, compile_with_cognition};

/// The READY record of the live request and its candidate, judged in its first compile by the
/// explicit approving double.
async fn judged_record() -> (Value, String) {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = Judging::new(&seat, approve);
    let out = compiled(&judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    (
        out.provenance.plan.clone().unwrap(),
        out.candidate.clone().unwrap(),
    )
}

/// What a held candidate offers once its judge located defects the round cannot repair from:
/// this verifier is not asked again on these bytes, in this compile or a later round carrying
/// the verdict.
const HELD_DEFECTS: &str = "The candidate was judged and not accepted: the parts named above stay missing. It is shown, never offered, and nothing was written; this verifier is not asked again on these bytes, in this compile or in a later round that carries this verdict. A correction of the request, another authoring model or another verifier can decide it.";
/// What a held candidate offers once its judge rejected it with no defect located.
const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";
/// What a held candidate offers once its judge abstained: an abstention is never carried to a
/// later round, so a new round that authors again can decide it.
const HELD_ABSTAINED: &str = "The verifier read the candidate and abstained: it neither accepted nor rejected it, and located no defect. It is shown, never offered, and nothing was written; it is not asked again on these bytes in this compile. A correction of the request, another verifier, or a new round that authors again can decide it.";

/// The `verify_held` findings of an outcome, in order.
fn held(out: &CompileOutcome) -> Vec<String> {
    (out.diagnostics.iter())
        .filter(|d| d.target == "verify_held")
        .map(|d| d.message.clone())
        .collect()
}

/// The verification attempt an answer round recorded.
fn attempt(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["semantic_verification"][0].clone()
}

/// The answer round of `record`, judged by `verdict` over a seat that must not author: the
/// outcome, the seat's calls and the number of questions the judge was asked.
async fn answered_by(record: &Value, verdict: JudgeVerdict) -> (CompileOutcome, usize, usize) {
    let seat = Scripted::new(vec![plan(SUM).to_string()]);
    let judge = Judging::new(&seat, verdict);
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
    let request = CompileRequest::create(intent(SUM))
        .with_knowledge(observed)
        .with_plan(record.clone())
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    let out = compile_with_provider(&request, &judge).await.unwrap();
    (out, seat.calls(), judge.judged().len())
}

/// An answer round whose own judge contradicts the record or leaves its remainder unapproved is
/// never READY (R4 A11, Q2, C3): the record a first compile judged READY replays its same bytes,
/// and the round asks its judge, with no plan, transform or repair call, the remainder the core
/// names (the clause no law reads), then the whole request, pending again on the replayed bytes
/// of a model's plan. A refusing judge finds the clause missing and names the task that fails
/// it, then the whole request unfaithful and each part missing with the task it names: defects,
/// each with its reason. An abstaining judge settles nothing: the clause, each part, whether a
/// task does more than asked and the whole request stay unknown. Either way the judge declined
/// these bytes: they stay the preview, never offered, and never replayed to it again (a refusal
/// rides the record it keeps; an abstention drops the record).
/// A model's plan record its judge rejected replays any number of times and never asks that
/// judge again (A3): each answer round repeats the rejection the record carries with no call,
/// and returns the record, its rejections still inside it, for the next round. The plan door
/// rebuilds the record of the plan it replays; the rejections ride the rebuilt record too.
#[tokio::test]
async fn a_rejected_plan_record_replays_twice_and_asks_its_judge_nothing_either_time() {
    let (record, candidate) = judged_record().await;
    let (first, authored, asked) = answered_by(&record, refuse).await;
    assert!(
        authored == 0 && asked > 0,
        "the judge rejects once: {first:#?}"
    );
    assert!(record_as_declined(&first), "{first:#?}");
    let mut kept = first.provenance.plan.clone().unwrap_or_default();
    for round in 1..=2 {
        let (out, authored, asked) = answered_by(&kept, refuse).await;
        assert_eq!((authored, asked), (0, 0), "round {round}: {out:#?}");
        assert_eq!(
            out.status,
            CompileStatus::Incomplete,
            "round {round}: {out:#?}"
        );
        assert_eq!(out.candidate.as_deref(), Some(candidate.as_str()));
        assert_eq!(attempt(&out)["carried"], true, "round {round}: {out:#?}");
        assert!(record_as_declined(&out), "round {round}: {out:#?}");
        kept = out.provenance.plan.clone().unwrap_or_default();
    }
}

/// A round with no judge between them changes nothing (A3): the rejected record a round replays
/// with no judge (cognition with no provider, or the core's own replay) keeps the rejections it
/// carries on the record it rebuilds, so the next round under the judge that rejected the bytes
/// repeats the rejection and asks it nothing.
#[tokio::test]
async fn a_rejected_record_replayed_with_no_judge_still_asks_its_judge_nothing() {
    let (record, candidate) = judged_record().await;
    let (first, _, asked) = answered_by(&record, refuse).await;
    assert!(asked > 0 && record_as_declined(&first), "{first:#?}");
    let kept = first.provenance.plan.clone().unwrap_or_default();
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
    let request = CompileRequest::create(intent(SUM))
        .with_knowledge(observed)
        .with_plan(kept.clone())
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: None,
    };
    let unjudged = [
        compile_with_cognition(&request, cognition).await.unwrap(),
        nika_compile::compile(&request).unwrap(),
    ];
    for (round, out) in unjudged.into_iter().enumerate() {
        assert_eq!(out.status, CompileStatus::Incomplete, "{round}: {out:#?}");
        assert_eq!(
            out.candidate.as_deref(),
            Some(candidate.as_str()),
            "{round}"
        );
        let carried = out.provenance.plan.clone().unwrap_or_default();
        assert_eq!(carried["declined"], kept["declined"], "{round}: {out:#?}");
        let (again, authored, asked) = answered_by(&carried, refuse).await;
        assert_eq!((authored, asked), (0, 0), "{round}: {again:#?}");
        assert_eq!(attempt(&again)["carried"], true, "{round}: {again:#?}");
        assert!(record_as_declined(&again), "{round}: {again:#?}");
    }
}

#[tokio::test]
async fn an_answer_round_whose_judge_refuses_the_remainder_is_incomplete() {
    let (record, candidate) = judged_record().await;
    let refused = format!(
        "it does not carry « {} (the judge points to the task compute) »",
        SUM.0
    );
    let abstained = format!("The judge could not settle `{}`", SUM.0);
    let refusing = [
        "judge_clause",
        "judge_point",
        "judge_request",
        "judge_part",
        "judge_point",
        "judge_part",
        "judge_point",
        "judge_part",
        "judge_point",
    ];
    let abstaining = [
        "judge_clause",
        "judge_request",
        "judge_part",
        "judge_part",
        "judge_part",
        "judge_extra",
    ];
    let cases: [(JudgeVerdict, &[&str], &str, &str); 2] = [
        (refuse, &refusing, refused.as_str(), HELD_DEFECTS),
        (abstain, &abstaining, abstained.as_str(), HELD_ABSTAINED),
    ];
    for (verdict, roles, named, words) in cases {
        let (out, authored, asked) = answered_by(&record, verdict).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert_eq!(out.candidate.as_deref(), Some(candidate.as_str()));
        assert_eq!(authored, 0, "{out:#?}");
        assert_eq!(judged(&out), roles, "{out:#?}");
        assert_eq!(asked, roles.len());
        assert!(record_as_declined(&out), "{named}: {out:#?}");
        assert_eq!(held(&out), [words], "{out:#?}");
        let route = out.provenance.decision.as_ref().unwrap()["route"].to_string();
        assert!(route.contains("verify: doubted, not replayable"), "{route}");
        let told = verifier_said(&out);
        assert!(told.iter().any(|m| m.contains(named)), "{told:?}");
    }
}

/// Nothing a record or a request carries is a judgment (R4 A11, Q2, labelled negatives): a
/// record forged with judged fields, and answers keyed as the judge's own questions, are never
/// READY; the plain replay emits the same bytes with zero calls, its remainder named.
#[tokio::test]
async fn a_forged_judgment_settles_nothing() {
    let (record, candidate) = judged_record().await;
    let mut forged = record.clone();
    forged["judgments"] = json!([{"clause": SUM.0, "disposition": "carried", "seat": "a/judge"}]);
    forged["semantic_verification"] = json!([{"defects": [], "unknown": []}]);
    let request = CompileRequest::create(intent(SUM)).with_plan(forged);
    let replayed = nika_compile::compile(&request).unwrap();
    assert_ne!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    let answered = CompileRequest::create(intent(SUM))
        .with_plan(record)
        .answer("verify-request", "\"faithful\"")
        .answer("verify-clause-0", "\"carried\"");
    let replayed = nika_compile::compile(&answered).unwrap();
    assert_ne!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert!(replayed.provenance.authoring.is_none());
    let plain = nika_compile::compile(
        &CompileRequest::create(intent(SUM)).with_plan(judged_record().await.0),
    )
    .unwrap();
    assert_eq!(plain.status, CompileStatus::Incomplete, "{plain:#?}");
    assert_eq!(plain.candidate.as_deref(), Some(candidate.as_str()));
}

/// Only a judgment bound to this request, plan and candidate settles its clause (R4 A11),
/// through the core's own replay door: the right judgments are READY (the positive control);
/// judgments bound to other bytes (stale), naming another span, or naming another clause settle
/// nothing, and a whole-request replay waits for its own judgment.
#[tokio::test]
async fn only_an_active_judgment_bound_to_its_candidate_settles_its_clause() {
    let (record, candidate) = judged_record().await;
    let text = intent(SUM);
    let request = CompileRequest::create(text.clone()).with_plan(record.clone());
    let replay = |judgments: &[Judgment], whole: bool| {
        let mut out = nika_compile::surface::initial();
        nika_compile::surface::replay_judged(&text, &record, &request, judgments, whole, &mut out)
            .unwrap();
        out
    };
    let unjudged = replay(&[], false);
    assert_eq!(unjudged.status, CompileStatus::Incomplete, "{unjudged:#?}");
    let open = unjudged.provenance.decision.as_ref().unwrap()["pending"]["open"].clone();
    let clauses: Vec<String> = open
        .as_array()
        .unwrap()
        .iter()
        .map(|duty| duty["clause"].as_str().unwrap().to_owned())
        .collect();
    assert!(!clauses.is_empty(), "{open:#}");
    let mut plan = Plan::from_json(&record).unwrap();
    promote_stated_rules(&mut plan, &text);
    let bound = Binding::of(&text, &request, &plan, &candidate);
    let judge = |clause: &str, binding: &Binding| {
        let at = text.find(clause).unwrap();
        let span = (at, at + clause.len());
        Judgment::new(
            clause,
            span,
            Disposition::Carried,
            "a/judge",
            "q",
            binding.clone(),
        )
    };
    let right: Vec<Judgment> = clauses.iter().map(|c| judge(c, &bound)).collect();
    assert_eq!(replay(&right, false).status, CompileStatus::Ready);
    let other = Binding::of(&text, &request, &plan, "nika: another-candidate\n");
    let stale: Vec<Judgment> = clauses.iter().map(|c| judge(c, &other)).collect();
    assert_eq!(replay(&stale, false).status, CompileStatus::Incomplete);
    let moved: Vec<Judgment> = right
        .iter()
        .cloned()
        .map(|mut j| {
            j.span = (0, 4);
            j
        })
        .collect();
    assert_eq!(replay(&moved, false).status, CompileStatus::Incomplete);
    let elsewhere = vec![judge(SUM.1, &bound)];
    assert_eq!(replay(&elsewhere, false).status, CompileStatus::Incomplete);
    assert_eq!(replay(&right, true).status, CompileStatus::Incomplete);
}

/// A judge double over a scripted seat (a labelled negative): it carries every clause asked
/// alone and its whole-request call fails with no answer, as a provider may, before or after
/// any transport; every other call goes to the seat unchanged.
struct FailsWhole<'a> {
    inner: &'a Scripted,
    failed: AtomicUsize,
}

/// The closed choices a request offers, in order; none for an open request.
fn offered(request: &InferRequest) -> Vec<String> {
    let ResponseFormat::JsonSchema(schema) = &request.response_format else {
        return Vec::new();
    };
    (schema["properties"]["choice"]["enum"]
        .as_array()
        .into_iter()
        .flatten())
    .filter_map(|key| key.as_str().map(str::to_owned))
    .collect()
}

impl ProviderInferDyn for FailsWhole<'_> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let keys = offered(&request);
        if !verifier(&keys) {
            return self.inner.infer(request).await;
        }
        if keys.iter().any(|key| key == "faithful") {
            self.failed.fetch_add(1, Ordering::SeqCst);
            return Err(ProviderError::Other {
                reason: "the provider failed with no answer".to_owned(),
            });
        }
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": approval(&keys)}).to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// Why the verification stopped at a call with no answer.
const STOPPED: &str = "The verification stopped at a judge call that got no answer (refused by the call bound, or failed: the receipt says which); nothing after it was asked of that judge. Next: another round, or a larger call bound.";

/// The first round of [`SUM`] whose whole-request call failed after its clause was carried: no
/// admitted answer declined the bytes, so the COLD record is kept for an answer round. The
/// outcome and its record.
async fn failed_whole() -> (CompileOutcome, Value) {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = FailsWhole {
        inner: &seat,
        failed: AtomicUsize::new(0),
    };
    let out = compiled_as(&judge, SUM, 0).await;
    assert_eq!(judge.failed.load(Ordering::SeqCst), 1, "{out:#?}");
    assert_eq!(seat.calls(), 2, "{out:#?}");
    let record = out.provenance.plan.clone().expect("the record is kept");
    (out, record)
}

/// A COLD round whose whole-request call failed keeps its record (nothing declined the bytes),
/// and its answer round asks the whole request again on the same bytes, with no authoring call:
/// the clause the core names, then the whole request, READY once the judge carries both.
#[tokio::test]
async fn a_cold_record_whose_whole_call_failed_asks_it_again_in_its_answer_round() {
    let (failed, record) = failed_whole().await;
    assert_eq!(failed.status, CompileStatus::Incomplete, "{failed:#?}");
    assert_eq!(judged(&failed), ["judge_clause", "judge_request"]);
    let first = attempt(&failed);
    let unknown = (&first["unknown"], &first["doubt"], &first["stopped"]);
    let request = json!([intent(SUM)]);
    assert_eq!(unknown, (&request, &json!([]), &json!(true)), "{first:#}");
    let calls = (&first["attempted"], &first["returned"], &first["declined"]);
    assert_eq!(calls, (&json!(2), &json!(1), &json!(false)), "{first:#}");
    assert_eq!(record["strategy"], "cold", "{record:#}");
    assert_eq!(held(&failed), Vec::<String>::new(), "{failed:#?}");
    let told = verifier_said(&failed);
    assert_eq!(told.last().map(String::as_str), Some(STOPPED), "{told:?}");
    let (answered, authored, _) = answered_by(&record, approve).await;
    assert_eq!(authored, 0, "{answered:#?}");
    assert_eq!(answered.candidate, failed.candidate, "the same bytes");
    assert_eq!(judged(&answered), ["judge_clause", "judge_request"]);
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
    let settled = attempt(&answered)["settled_by"].clone();
    assert_eq!(settled, json!("verify-request"), "{answered:#?}");
}

/// The verdict of a judge that carries every clause and part, names no extra task, and answers
/// the whole request `answer`.
fn whole_answered(asked: &Asked<'_>, answer: &str) -> String {
    if asked.offers("faithful") {
        answer.to_owned()
    } else {
        approve(asked)
    }
}

/// The verdict of a judge that finds the whole request unfaithful, all else approved.
fn unfaithful_whole(asked: &Asked<'_>) -> String {
    whole_answered(asked, "unfaithful")
}

/// The verdict of a judge that abstains on the whole request, all else approved.
fn abstaining_whole(asked: &Asked<'_>) -> String {
    whole_answered(asked, "none")
}

/// That answer round is READY only on a faithful whole request: the same judge carrying the
/// clause and each part but answering the whole request `unfaithful` contests it with no trial
/// run to decide it, and one abstaining on it leaves it unknown. Neither is READY; each declined
/// the bytes, so they stay the preview, held, and the record is dropped.
#[tokio::test]
async fn a_cold_answer_round_is_ready_only_on_a_faithful_whole_request() {
    let (failed, record) = failed_whole().await;
    let roles = [
        "judge_clause",
        "judge_request",
        "judge_part",
        "judge_part",
        "judge_part",
        "judge_extra",
    ];
    let whole = json!([intent(SUM)]);
    let none = json!([]);
    let cases: [(JudgeVerdict, &str, (&Value, &Value), &str); 2] = [
        (unfaithful_whole, "unfaithful", (&whole, &none), HELD),
        (abstaining_whole, "none", (&none, &whole), HELD_ABSTAINED),
    ];
    for (verdict, answer, (contested, unknown), words) in cases {
        let (out, authored, _) = answered_by(&record, verdict).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{answer}: {out:#?}");
        assert_eq!(out.candidate, failed.candidate, "{answer}: the same bytes");
        assert_eq!(authored, 0, "{answer}: {out:#?}");
        assert_eq!(judged(&out), roles, "{answer}: {out:#?}");
        let recorded = attempt(&out);
        assert_eq!(recorded["doubt"], json!([answer]), "{recorded:#}");
        let open = (&recorded["contested"], &recorded["unknown"]);
        assert_eq!(open, (contested, unknown), "{recorded:#}");
        assert_eq!(recorded["settled_by"], Value::Null, "{recorded:#}");
        assert!(record_as_declined(&out), "{answer}: {out:#?}");
        assert_eq!(held(&out), [words], "{answer}: {out:#?}");
    }
}

/// With no judge (an authoring policy, but neither its provider nor a decision seat), that
/// answer round replays the same bytes and stays INCOMPLETE: the clause the core names and the
/// whole request are pending on them, each named, and the record is kept.
#[tokio::test]
async fn a_cold_answer_round_with_no_judge_stays_pending_on_the_whole_request() {
    let (failed, record) = failed_whole().await;
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
    let request = CompileRequest::create(intent(SUM))
        .with_plan(record.clone())
        .with_authoring_policy(policy);
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: None,
    };
    let out = compile_with_cognition(&request, cognition).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(out.candidate, failed.candidate, "the same bytes");
    assert!(out.provenance.authoring.is_none(), "{out:#?}");
    assert_eq!(out.provenance.plan.as_ref(), Some(&record), "{out:#?}");
    let text = intent(SUM);
    let pending = &out.provenance.decision.as_ref().unwrap()["pending"]["open"];
    let clauses: Vec<&Value> = (pending.as_array().into_iter().flatten())
        .map(|duty| &duty["clause"])
        .collect();
    assert_eq!(clauses, [&json!(SUM.0), &json!(text)], "{pending:#}");
    let whole = json!({"clause": text, "witness": null, "spans": [[0, text.len()]]});
    assert_eq!(pending[1], whole, "{pending:#}");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    let sha = nika_compile::surface::sha256(candidate);
    let named = format!(
        "The request states `{text}` and no element of the plan names it: no law reads from candidate {} that it carries it, and no admitted judgment made in this compile settles it. Nothing is READY on a pending clause: it stays INCOMPLETE until an admitted judgment of these bytes against the whole request carries it.",
        &sha[..12]
    );
    assert!(verifier_said(&out).contains(&named), "{out:#?}");
}

/// Fail closed (R4 A11): a record whose strategy word is missing or unknown is no plan of the
/// reader's own HOT door, so its judgeless answer round keeps the whole request pending on the
/// replayed bytes, as a model's plan does: INCOMPLETE, the whole request named, never READY on
/// bytes no judge carried.
#[tokio::test]
async fn a_record_with_no_or_an_unknown_strategy_word_keeps_the_whole_request_pending() {
    let (failed, record) = failed_whole().await;
    let text = intent(SUM);
    for strategy in [Value::Null, json!("tepid")] {
        let mut kept = record.clone();
        let fields = kept.as_object_mut().unwrap();
        match strategy.as_str() {
            Some(word) => fields.insert("strategy".to_owned(), json!(word)),
            None => fields.remove("strategy"),
        };
        let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
        let request = CompileRequest::create(text.as_str())
            .with_plan(kept)
            .with_authoring_policy(policy);
        let cognition = Cognition::<NoProvider> {
            provider: None,
            seat: None,
        };
        let out = compile_with_cognition(&request, cognition).await.unwrap();
        assert_eq!(
            out.status,
            CompileStatus::Incomplete,
            "{strategy}: {out:#?}"
        );
        assert_eq!(
            out.candidate, failed.candidate,
            "{strategy}: the same bytes"
        );
        assert!(out.provenance.authoring.is_none(), "{strategy}: {out:#?}");
        let pending = &out.provenance.decision.as_ref().unwrap()["pending"]["open"];
        let whole = json!({"clause": text, "witness": null, "spans": [[0, text.len()]]});
        let open = pending.as_array().into_iter().flatten();
        assert!(
            open.into_iter().any(|duty| *duty == whole),
            "{strategy}: {pending:#}"
        );
    }
}
