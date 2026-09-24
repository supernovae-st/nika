// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answer round replays the plan the previous round produced: the same candidate, the
//! same questions, zero provider and zero seat calls. A plan that does not parse, is not
//! anchored in the intent or still carries unknowns is a finding, never a candidate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_compile::{
    AuthoringCognition, AuthoringPolicy, CompileRequest, CompileStatus, DiagnosticKind, Strategy,
    compile, intent_sha256, outcome_document,
};
use nika_compile_cognition::{
    Cognition, compile_with_cognition, compile_with_provider,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat},
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
const COLD_INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";
/// Every clause explicit: the deterministic reader admits it without any seat.
const HOT_INTENT: &str = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";

struct Provider {
    text: String,
    calls: AtomicU32,
}
impl Provider {
    fn new(plan: &Value) -> Self {
        Self {
            text: plan.to_string(),
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Provider {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            TokenUsage::new(120, 90),
            StopReason::EndTurn,
        ))
    }
}

/// A seat that must never be asked during a replay.
struct Seat(AtomicU32);
impl DecisionSeat for Seat {
    fn name(&self) -> &'static str {
        "double/seat"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let key = question.options[0].key.clone();
        Box::pin(async move { Ok(ChoiceAnswer::new(key, "double/seat")) })
    }
}

fn proposal() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
}
fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
}
fn answered(request: CompileRequest) -> CompileRequest {
    request
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer(
            "const.refund_policy",
            r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
        )
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
}
fn keys(out: &nika_compile::CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}
fn route(out: &nika_compile::CompileOutcome) -> Value {
    outcome_document(out)["provenance"]["decision"]["route"].clone()
}

#[tokio::test]
async fn a_recorded_cold_plan_replays_the_same_candidate_with_zero_calls() {
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
    // The original answer round: the provider is called again and may drift.
    let original = compile_with_provider(
        &answered(CompileRequest::create(COLD_INTENT).with_authoring_policy(policy())),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(original.status, CompileStatus::Ready, "{original:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    // The replayed answer round: the recorded plan, the same answers, no seat of any kind.
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
    assert_eq!(seat.0.load(Ordering::SeqCst), 0, "zero seat calls");
    assert_eq!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert_eq!(replayed.candidate, original.candidate);
    assert_eq!(replayed.provenance.strategy, Some(Strategy::Cold));
    assert_eq!(replayed.provenance.authoring, None);
    assert_eq!(
        replayed.provenance.cognition,
        AuthoringCognition::DeterministicOnly
    );
    assert_eq!(route(&replayed), json!(["replayed plan"]));
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
    // An unanswered replay asks exactly the questions the fresh compile asked.
    let unanswered = compile(&CompileRequest::create(COLD_INTENT).with_plan(plan)).unwrap();
    assert_eq!(keys(&unanswered), keys(&first));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_recorded_hot_plan_replays_identically() {
    let fresh = compile(&CompileRequest::create(HOT_INTENT)).unwrap();
    assert_eq!(fresh.provenance.strategy, Some(Strategy::Hot), "{fresh:#?}");
    let plan = fresh.provenance.plan.clone().unwrap();
    assert_eq!(plan["strategy"], "hot");
    assert_eq!(
        outcome_document(&fresh)["provenance"]["decision"]["intent_sha256"],
        intent_sha256(HOT_INTENT)
    );
    let replayed = compile(&CompileRequest::create(HOT_INTENT).with_plan(plan.clone())).unwrap();
    assert_eq!(replayed.status, fresh.status);
    assert_eq!(replayed.candidate, fresh.candidate);
    assert_eq!(keys(&replayed), keys(&fresh));
    assert_eq!(replayed.provenance.strategy, Some(Strategy::Hot));
    assert_eq!(route(&replayed), json!(["replayed plan"]));
    let ready = compile(
        &CompileRequest::create(HOT_INTENT)
            .with_plan(plan)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    let expected =
        compile(&CompileRequest::create(HOT_INTENT).answer("model", r#""mock/echo""#)).unwrap();
    assert_eq!(ready.candidate, expected.candidate);
}

#[test]
fn a_malformed_or_unanchored_plan_is_a_finding_never_a_candidate() {
    let good = compile(&CompileRequest::create(HOT_INTENT))
        .unwrap()
        .provenance
        .plan
        .unwrap();
    let mut unanchored = good.clone();
    unanchored["operations"][0]["evidence"] = json!("evidence the request never wrote");
    let mut unknown_op = good.clone();
    unknown_op["operations"][0]["op"] = json!("teleport");
    let mut unknown_role = good.clone();
    unknown_role["bindings"] = json!([{"role":"invented","literal":"x"}]);
    let mut bad_policy = good.clone();
    bad_policy["effects"][0]["policy"] = json!("whenever");
    let mut bad_obligation = good.clone();
    bad_obligation["obligations"] = json!([{"kind":"retry_bound","value":null,"evidence":"Read"}]);
    for plan in [
        json!("not an object"),
        json!({}),
        json!({"operations":"nope"}),
        json!({"operations":[{"op":"read"}],"effects":[],"obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":null}),
        unanchored,
        unknown_op,
        unknown_role,
        bad_policy,
        bad_obligation,
    ] {
        let out = compile(&CompileRequest::create(HOT_INTENT).with_plan(plan.clone())).unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{plan}");
        assert!(out.candidate.is_none(), "{plan}: {out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| { d.kind == DiagnosticKind::Unknown && d.target == "recorded_plan" }),
            "{plan}: {out:#?}"
        );
        assert_eq!(route(&out), json!(["replayed plan"]), "{plan}");
    }
}

#[test]
fn a_recorded_plan_with_unknowns_is_never_assembled() {
    let mut plan = compile(&CompileRequest::create(HOT_INTENT))
        .unwrap()
        .provenance
        .plan
        .unwrap();
    plan["unknowns"] = json!(["Use a previous approval for a different amount"]);
    let out = compile(
        &CompileRequest::create(HOT_INTENT)
            .with_plan(plan)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
}

#[test]
fn the_plan_record_round_trips_every_element() {
    // Bindings, constraints, categories, obligations with a value and a trigger all survive.
    let intent = "Read ./notes/brief.md, classify it as urgent or routine, and write the verdict to ./out/verdict.md";
    let fresh = compile(&CompileRequest::create(intent)).unwrap();
    let plan = fresh.provenance.plan.clone().expect("plan");
    let replayed = compile(&CompileRequest::create(intent).with_plan(plan.clone())).unwrap();
    assert_eq!(replayed.provenance.plan, Some(plan.clone()));
    assert_eq!(replayed.candidate, fresh.candidate);
    let mut with_extras = plan;
    with_extras["obligations"] = json!([{"kind":"retry_bound","value":3,"evidence":"Read"},{"kind":"dedup","value":null,"evidence":"classify"}]);
    with_extras["constraints"] = json!(["never infer the priority"]);
    with_extras["trigger"] = json!("every morning");
    with_extras["bindings"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"timezone","literal":"Europe/Paris"}));
    let out = compile(&CompileRequest::create(intent).with_plan(with_extras.clone())).unwrap();
    let recorded = out.provenance.plan.expect("plan");
    for key in [
        "operations",
        "effects",
        "obligations",
        "bindings",
        "constraints",
        "unknowns",
        "trigger",
    ] {
        assert_eq!(recorded[key], with_extras[key], "{key}");
    }
}

#[test]
fn intent_sha_folds_apostrophes_and_is_hex() {
    let straight = intent_sha256("Lis l'avis, puis résume-le");
    let curly = intent_sha256("Lis l’avis, puis résume-le");
    assert_eq!(straight, curly);
    assert_eq!(straight.len(), 64);
    assert!(straight.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(straight, intent_sha256("Lis l'avis"));
}

#[test]
fn a_replayed_document_keeps_the_generation_one_shape() {
    let plan = compile(&CompileRequest::create(HOT_INTENT))
        .unwrap()
        .provenance
        .plan
        .unwrap();
    let out = compile(&CompileRequest::create(HOT_INTENT).with_plan(plan)).unwrap();
    let document = outcome_document(&out);
    let keys: Vec<_> = document.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "candidate",
            "check_preview",
            "compile_version",
            "diagnostics",
            "provenance",
            "questions",
            "requested_boundary",
            "requested_trigger",
            "status"
        ]
    );
    let provenance: Vec<_> = document["provenance"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !matches!(k.as_str(), "strategy" | "plan" | "decision"))
        .cloned()
        .collect();
    // `suggested_file` joined the provenance additively (a file name for whoever saves the
    // candidate); the generation stays one.
    assert_eq!(
        provenance,
        [
            "cognition",
            "compiler_version",
            "skeleton",
            "spec_pin",
            "suggested_file"
        ]
    );
}
