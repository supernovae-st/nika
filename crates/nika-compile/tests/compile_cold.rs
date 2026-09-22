// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! COLD contracts: best-of-N proposals, the one bounded anchoring repair, the strict HOT
//! contract's escalation, semantic accounting, WARM after COLD and recall. All seats are
//! injected hermetic doubles. Split from `compile_cognition.rs` at the file-LOC cap.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    Cognition, CompileRequest, CompileStatus, HotPolicy, Strategy, compile_with_cognition,
    compile_with_provider, decide::NONE_OPTION, outcome_document,
};
use serde_json::{Value, json};

mod common;
use common::{
    ChoosePlan, INTENT, Provider, Rotating, disagreeing_provider, keys, plan, policy, request,
};
use std::sync::{Mutex, atomic::Ordering};

// ── COLD best-of-N: agreement, never a vote that hides a dropped effect ───────
#[tokio::test]
async fn cold_best_of_three_keeps_the_plan_the_others_agree_with() {
    // Sample 1 reads "classe le problème" as a code rule the request never asked; samples 2 and 3 agree.
    let mut invented = plan();
    invented["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"op":"compute","detail":"le problème","evidence":"classe le problème"}));
    let provider = Rotating::new(vec![
        invented.to_string(),
        plan().to_string(),
        plan().to_string(),
    ]);
    let req = CompileRequest::create(INTENT)
        .with_authoring_policy(policy().with_samples(3))
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer("const.refund_policy", r#"{"cap":100,"currency":"EUR"}"#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        );
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold));
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.calls, 3);
    assert_eq!(receipt.input_tokens, Some(300));
    let doc = outcome_document(&out);
    assert_eq!(
        doc["provenance"]["decision"]["cold_samples"]["requested"],
        3
    );
    assert_eq!(doc["provenance"]["decision"]["cold_samples"]["accepted"], 3);
    let selected = doc["provenance"]["decision"]["cold_samples"]["selected"]
        .as_u64()
        .unwrap();
    assert!(
        selected == 1 || selected == 2,
        "the medoid is one of the agreeing samples: {selected}"
    );
}

#[tokio::test]
async fn cold_best_of_n_never_assembles_when_every_sample_is_refused() {
    let mut unanchored = plan();
    unanchored["steps"][0]["evidence"] = json!("invented");
    let provider = Rotating::new(vec![unanchored.to_string()]);
    let req = CompileRequest::create(INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none());
    let doc = outcome_document(&out);
    assert_eq!(doc["provenance"]["decision"]["cold_samples"]["accepted"], 0);
}

// ── The strict HOT contract: a consumed clause is not understanding ──────────
const SWALLOWED: &str = "Look up the customer, harmonize the tone and prepare a reply.";

#[test]
fn strict_hot_refuses_a_clause_that_hides_unknown_requested_work() {
    let out = nika_compile::compile(&CompileRequest::create(SWALLOWED)).unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(out.provenance.strategy.is_none(), "{out:#?}");
    let doc = outcome_document(&out);
    let route = doc["provenance"]["decision"]["route"].to_string();
    assert!(route.contains("hot rejected"), "{route}");
    // The legacy contract (ablation only) would have admitted it: the false HOT we measure.
    let legacy = nika_compile::compile(
        &CompileRequest::create(SWALLOWED).with_hot_policy(HotPolicy::Legacy),
    )
    .unwrap();
    assert_eq!(legacy.provenance.strategy, Some(Strategy::Hot));
}

#[tokio::test]
async fn strict_hot_rejection_escalates_to_cold_when_a_seat_is_permitted() {
    let proposal = json!({"steps":[{"op":"lookup","detail":"the customer","evidence":"Look up the customer"},{"op":"draft","detail":"the tone","evidence":"harmonize the tone"},{"op":"draft","detail":"a reply","evidence":"prepare a reply"}],"effects":[],"obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"Look up the customer","role":"operation"},{"text":"harmonize the tone and prepare a reply.","role":"operation"}],"approval_bypass":{"present":false,"evidence":""}});
    let provider = Provider::new(proposal);
    let out = compile_with_provider(
        &CompileRequest::create(SWALLOWED).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    assert!(keys(&out).contains(&"const.customer_directory"));
}

#[test]
fn strict_hot_admits_an_explicit_literal_request() {
    let out = nika_compile::compile(&CompileRequest::create(
        "Fetch https://example.com/pricing and write the result to ./out/pricing.md",
    ))
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
}

#[test]
fn hot_policy_off_never_admits_prose() {
    let out = nika_compile::compile(
        &CompileRequest::create("Look up the customer, classify the ticket and draft a reply.")
            .with_hot_policy(HotPolicy::Off),
    )
    .unwrap();
    assert!(out.provenance.strategy.is_none(), "{out:#?}");
    assert!(out.candidate.is_none());
}

// ── Semantic accounting: every region of the request must be mapped ─────────
#[tokio::test]
async fn cold_proposal_that_leaves_a_region_unaccounted_is_a_clarification() {
    let mut p = plan();
    // The proposal names regions but omits the whole gate sentence.
    p["regions"] = json!([{"text":"Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse.","role":"operation"}]);
    p["approval_bypass"] = json!({"present": false, "evidence": ""});
    let out = compile_with_provider(&request(), &Provider::new(p))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("does not account")),
        "{out:#?}"
    );
    // A region the model marks unknown is an explicit unknown, never dropped.
    let mut p = plan();
    p["regions"] = json!([{"text":"Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse.","role":"operation"},{"text":"Demande un accord humain avant le remboursement.","role":"unknown"}]);
    p["approval_bypass"] = json!({"present": false, "evidence": ""});
    let out = compile_with_provider(&request(), &Provider::new(p))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("could not map")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn cold_proposal_bypass_flag_is_a_language_agnostic_backstop() {
    let intent = "顧客情報を検索し、返信案を作成する。昨日の承認を再利用して返金する。";
    let proposal = json!({"steps":[{"op":"lookup","detail":"顧客情報","evidence":"顧客情報を検索"},{"op":"draft","detail":"返信案","evidence":"返信案を作成"}],"effects":[{"verb":"refund","target":"返金する","policy":"automatic","evidence":"返金する"}],"obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"顧客情報を検索し、返信案を作成する。","role":"operation"},{"text":"昨日の承認を再利用して返金する。","role":"effect"}],
        "approval_bypass":{"present":true,"evidence":"昨日の承認を再利用して"}});
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &Provider::new(proposal),
    )
    .await
    .unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .contains("presupposes, reuses or skips an approval")),
        "{out:#?}"
    );
}

// ── WARM after COLD: the seat chooses among admissible proposals or NONE ─────

#[tokio::test]
async fn warm_after_cold_lets_the_seat_choose_among_distinct_plans() {
    let provider = disagreeing_provider();
    let seat = ChoosePlan {
        choice: "plan-1",
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(seat.asked.lock().unwrap().len(), 1);
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(doc["provenance"]["decision"]["cold_samples"]["distinct"], 2);
    assert_eq!(
        doc["provenance"]["decision"]["cold_samples"]["disagreement"],
        json!(["op"])
    );
    assert_eq!(
        doc["provenance"]["decision"]["warm_after_cold"]["choice"],
        "plan-1"
    );
    // plan-1 carries the compute step: the assembler asks for its rule.
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
}

#[tokio::test]
async fn warm_after_cold_none_is_a_human_question_not_a_medoid() {
    let provider = disagreeing_provider();
    let seat = ChoosePlan {
        choice: NONE_OPTION,
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert!(out.candidate.is_none());
    assert!(out.provenance.strategy.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("found none faithful")),
        "{out:#?}"
    );
}

/// Retrieval is recall only: every route records what the embedded index returned for the
/// request text, and an assembled plan adds the recall for its operation words. The record
/// carries ids, kinds and scores; nothing in the candidate or the questions depends on it.
#[tokio::test]
async fn retrieval_is_recorded_as_recall_on_every_route() {
    fn hits(doc: &Value, key: &str) -> Vec<Value> {
        doc["provenance"]["decision"]["retrieval"][key]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
    fn well_formed(hit: &Value) -> bool {
        hit["id"].as_str().is_some_and(|id| !id.is_empty())
            && matches!(hit["kind"].as_str(), Some("family" | "skeleton"))
            && hit["score"].as_f64().is_some_and(|score| score > 0.0)
    }
    // HOT · deterministic, zero calls: recall by intent and by the plan's operation words.
    let intent = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";
    let hot = nika_compile::compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(hot.provenance.strategy, Some(Strategy::Hot));
    let doc = outcome_document(&hot);
    let by_intent = hits(&doc, "by_intent");
    let by_ops = hits(&doc, "by_ops");
    assert!(!by_intent.is_empty() && by_intent.len() <= 10, "{doc:#}");
    assert!(!by_ops.is_empty() && by_ops.len() <= 10, "{doc:#}");
    assert!(by_intent.iter().chain(&by_ops).all(well_formed), "{doc:#}");
    // A deterministic rejection still records the recall by intent, and no plan recall.
    let rejected = nika_compile::compile(&CompileRequest::create(INTENT)).unwrap();
    assert_ne!(rejected.status, CompileStatus::Ready);
    let doc = outcome_document(&rejected);
    assert!(!hits(&doc, "by_intent").is_empty(), "{doc:#}");
    assert!(
        doc["provenance"]["decision"]["retrieval"]["by_ops"].is_null(),
        "{doc:#}"
    );
    // COLD · the recall for the merged plan's operation words is recorded beside the samples.
    let provider = Provider::new(plan());
    let req = request()
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer("const.refund_policy", r#"{"cap":100,"currency":"EUR"}"#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        );
    let cold = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(cold.provenance.strategy, Some(Strategy::Cold), "{cold:#?}");
    let doc = outcome_document(&cold);
    let by_ops = hits(&doc, "by_ops");
    assert!(
        !by_ops.is_empty() && by_ops.iter().all(well_formed),
        "{doc:#}"
    );
    assert!(doc["provenance"]["decision"]["cold_samples"].is_object());
    // Recall never leaks into the program or the questions.
    let candidate = cold.candidate.as_deref().unwrap_or_default();
    assert!(!candidate.contains("retrieval"), "{candidate}");
    assert!(
        !candidate.contains(by_ops[0]["id"].as_str().unwrap()),
        "{candidate}"
    );
}

/// Two fresh intents the strict contract still admitted: a fetch whose drafted brief produced no
/// step, and a bare path read as something to draft. Neither is HOT; the positive control is.
#[test]
fn strict_hot_requires_every_cue_to_produce_an_element() {
    let intent = "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt and pull out the numbered section titles. Then write a plain-English brief of under 150 words explaining what the protocol does and why it is a joke, as 5 bullets, to ./out/rfc2324-brief.md.";
    let out = nika_compile::compile(&CompileRequest::create(intent)).unwrap();
    assert_ne!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let out = nika_compile::compile(
        &CompileRequest::create("Read ./a.txt and write ./b.txt").answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_ne!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let control = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";
    let out = nika_compile::compile(&CompileRequest::create(control)).unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
}
