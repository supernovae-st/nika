// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Explicit authoring provider contracts. All providers are injected hermetic doubles.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use nika_onboard::compile::{
    AuthoringPolicy, CompileRequest, CompileStatus, compile_with_provider, outcome_document,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

struct Provider {
    text: String,
    calls: AtomicU32,
}
impl Provider {
    fn new(plan: impl std::fmt::Display) -> Self {
        Self {
            text: plan.to_string(),
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Provider {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.max_tokens, Some(1024));
        assert_eq!(request.timeout, Some(Duration::from_secs(2)));
        assert!(request.tools.is_empty());
        assert!(request.temperature.is_none());
        assert!(request.extra.params.is_empty());
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            TokenUsage::new(120, 90),
            StopReason::EndTurn,
        ))
    }
}
const INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis prépare une réponse. Demande un accord humain avant le remboursement.";
fn plan() -> Value {
    json!({"steps":[{"operation":"lookup","evidence":"consulte le client"},{"operation":"classify","evidence":"classe le problème"},{"operation":"draft","evidence":"prépare une réponse"}],"effect":"human_first_refund","effect_evidence":"Demande un accord humain avant le remboursement","unknowns":[]})
}
fn request() -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(AuthoringPolicy::new(
        "mock/authoring",
        1024,
        Duration::from_secs(2),
    ))
}

#[tokio::test]
async fn provider_opt_in_returns_real_questions_and_versioned_usage() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.questions.iter().any(|q| q.key == "const.refund_policy"));
    let doc = outcome_document(&out);
    assert_eq!(doc["compile_version"], 2);
    assert_eq!(doc["provenance"]["cognition"], "explicitProvider");
    assert_eq!(doc["provenance"]["authoring"]["calls"], 1);
    assert_eq!(
        doc["provenance"]["authoring"]["sampling"],
        json!({"temperature":null,"seed":null,"effective":"providerDefaultUnknown"})
    );
    assert_eq!(doc["provenance"]["authoring"]["input_tokens"], 120);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let out = compile_with_provider(
        &request()
            .answer("model", r#""mock/echo""#)
            .answer("const.customer_directory", r#""customers.json""#)
            .answer(
                "const.refund_policy",
                r#""Return unused goods within 14 days; human must verify the receipt.""#,
            )
            .answer(
                "const.refund_endpoint",
                r#""https://refund.example.invalid/refund""#,
            ),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.check_preview.unwrap().report.is_clean());
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        2,
        "one per explicit recompile, not a hidden retry"
    );
}

#[tokio::test]
async fn no_opt_in_and_exact_skeleton_never_call_provider_or_change_v1() {
    let provider = Provider::new(plan());
    for req in [
        CompileRequest::create(INTENT),
        CompileRequest::create("hello").with_authoring_policy(AuthoringPolicy::new(
            "mock/authoring",
            1024,
            Duration::from_secs(2),
        )),
    ] {
        let out = compile_with_provider(&req, &provider).await.unwrap();
        let doc = outcome_document(&out);
        assert_eq!(doc["compile_version"], 1);
        assert!(doc["provenance"].get("authoring").is_none());
    }
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn malformed_unknown_conflicting_and_unanchored_plans_do_not_emit_source() {
    let mut cases = vec![json!({"yaml":"nika: invented"})];
    for effect in [
        "automatic_refund",
        "conflict",
        "unsupported",
        "forbidden",
        "send",
        "publish",
    ] {
        let mut p = plan();
        p["effect"] = json!(effect);
        cases.push(p);
    }
    for field in ["operation", "evidence"] {
        let mut p = plan();
        p["steps"][0][field] = json!("invented");
        cases.push(p);
    }
    let mut p = plan();
    p["unknowns"] = json!(["Use a previous approval for a different amount"]);
    cases.push(p);
    let mut p = plan();
    p["effect_evidence"] = json!("unmentioned refund authority");
    cases.push(p);
    for p in cases {
        let provider = Provider::new(p);
        let out = compile_with_provider(&request(), &provider).await.unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete);
        assert!(out.candidate.is_none());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn invalid_budget_never_calls_provider() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(
        &CompileRequest::create(INTENT).with_authoring_policy(AuthoringPolicy::new(
            "mock/authoring",
            0,
            Duration::from_secs(2),
        )),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn clarification_is_explicit_request_data_and_is_consumed_on_recompile() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(
        &request().answer(
            "intent.clarification",
            serde_json::to_string(INTENT).unwrap(),
        ),
        &provider,
    )
    .await
    .unwrap();
    assert!(out.questions.iter().any(|q| q.key == "const.refund_policy"));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.target == "intent.clarification")
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

struct Pending;
impl ProviderInferDyn for Pending {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        std::future::pending().await
    }
}
#[tokio::test]
async fn authoring_timeout_is_bounded_and_never_retries() {
    let req = CompileRequest::create(INTENT).with_authoring_policy(AuthoringPolicy::new(
        "mock/authoring",
        1024,
        Duration::from_millis(1),
    ));
    let out = compile_with_provider(&req, &Pending).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(out.provenance.authoring.unwrap().calls, 1);
    assert!(out.candidate.is_none());
}

#[tokio::test]
async fn model_cannot_omit_refund_or_insert_approval_into_automatic_refund() {
    let mut omitted = plan();
    omitted["effect"] = json!("none");
    omitted["effect_evidence"] = json!("");
    let out = compile_with_provider(&request(), &Provider::new(omitted))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification")
    );
    let intent = format!("{INTENT} Rembourse automatiquement sans mon accord.");
    let req = CompileRequest::create(intent).with_authoring_policy(AuthoringPolicy::new(
        "mock/authoring",
        1024,
        Duration::from_secs(2),
    ));
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification")
    );
}

#[tokio::test]
async fn recognized_send_omission_never_becomes_a_draft_only_candidate() {
    let mut proposal = plan();
    proposal["effect"] = json!("none");
    proposal["effect_evidence"] = json!("");
    let intent =
        "Consulte le client, classe le problème, prépare une réponse et envoyer la réponse.";
    let req = CompileRequest::create(intent).with_authoring_policy(AuthoringPolicy::new(
        "mock/authoring",
        1024,
        Duration::from_secs(2),
    ));
    let out = compile_with_provider(&req, &Provider::new(proposal))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification")
    );
}

#[tokio::test]
async fn unfamiliar_language_is_not_a_deterministic_contradiction() {
    for (intent, lookup, classify, draft, effect) in [
        (
            "读取客户资料，分类工单并起草回复。任何退款前先获得我的确认。",
            "读取客户资料",
            "分类工单",
            "起草回复",
            "任何退款前先获得我的确认",
        ),
        (
            "Consulta los datos del cliente, clasifica la solicitud y redacta una respuesta. Antes de devolver dinero, pide mi autorización.",
            "Consulta los datos del cliente",
            "clasifica la solicitud",
            "redacta una respuesta",
            "Antes de devolver dinero, pide mi autorización",
        ),
        (
            "顧客情報を検索し、問い合わせを分類して返信案を作成する。返金する前に私の承認を求める。",
            "顧客情報を検索",
            "問い合わせを分類",
            "返信案を作成",
            "返金する前に私の承認を求める",
        ),
    ] {
        let proposal = json!({"steps":[{"operation":"lookup","evidence":lookup},{"operation":"classify","evidence":classify},{"operation":"draft","evidence":draft}],"effect":"human_first_refund","effect_evidence":effect,"unknowns":[]});
        let req = CompileRequest::create(intent).with_authoring_policy(AuthoringPolicy::new(
            "mock/authoring",
            1024,
            Duration::from_secs(2),
        ));
        let out = compile_with_provider(&req, &Provider::new(proposal))
            .await
            .unwrap();
        assert!(
            out.questions.iter().any(|q| q.key == "const.refund_policy"),
            "{out:#?}"
        );
        assert!(
            !out.questions
                .iter()
                .any(|q| q.key == "intent.clarification")
        );
    }
}

#[tokio::test]
async fn complete_explicit_replacement_resolves_old_automatic_refund_request() {
    let req = CompileRequest::create(format!("{INTENT} Refund automatically."))
        .with_authoring_policy(AuthoringPolicy::new(
            "mock/authoring",
            1024,
            Duration::from_secs(2),
        ))
        .answer(
            "intent.clarification",
            serde_json::to_string(INTENT).unwrap(),
        );
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(
        out.questions.iter().any(|q| q.key == "const.refund_policy"),
        "{out:#?}"
    );
    assert!(
        !out.questions
            .iter()
            .any(|q| q.key == "intent.clarification")
    );
}

#[tokio::test]
async fn replacement_fragment_does_not_inherit_unstated_operations() {
    let req = request().answer(
        "intent.clarification",
        r#""Instead require fresh approval.""#,
    );
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(!out.questions.iter().any(|q| q.key == "const.refund_policy"));
}

#[tokio::test]
async fn approval_bypass_phrase_with_effect_none_is_a_recognized_omission() {
    for (intent, lookup, draft) in [
        (
            "Look up the customer, draft a reply, and issue store credits without asking me.",
            "Look up the customer",
            "draft a reply",
        ),
        (
            "Consulte le client, prépare une réponse et crédite le compte sans mon accord.",
            "Consulte le client",
            "prépare une réponse",
        ),
        (
            "Look up the customer, draft a reply, and reuse yesterday's approval for store credits.",
            "Look up the customer",
            "draft a reply",
        ),
    ] {
        let proposal = json!({"steps":[{"operation":"lookup","evidence":lookup},{"operation":"draft","evidence":draft}],"effect":"none","effect_evidence":"","unknowns":[]});
        let req = CompileRequest::create(intent).with_authoring_policy(AuthoringPolicy::new(
            "mock/authoring",
            1024,
            Duration::from_secs(2),
        ));
        let out = compile_with_provider(&req, &Provider::new(proposal))
            .await
            .unwrap();
        assert!(out.candidate.is_none(), "{intent}");
        assert!(
            out.questions
                .iter()
                .any(|q| q.key == "intent.clarification"),
            "{intent}: {out:#?}"
        );
        assert!(
            !out.questions
                .iter()
                .any(|q| q.key == "const.customer_directory"),
            "{intent}: a dropped effect must not become a lookup+draft candidate"
        );
    }
}

#[tokio::test]
async fn french_fichier_is_not_the_yesterday_bypass_phrase() {
    let intent = "Consulte le fichier client, classe le problème, prépare une réponse. Demande un accord humain avant le remboursement.";
    let proposal = json!({"steps":[{"operation":"lookup","evidence":"Consulte le fichier client"},{"operation":"classify","evidence":"classe le problème"},{"operation":"draft","evidence":"prépare une réponse"}],"effect":"human_first_refund","effect_evidence":"Demande un accord humain avant le remboursement","unknowns":[]});
    let req = CompileRequest::create(intent).with_authoring_policy(AuthoringPolicy::new(
        "mock/authoring",
        1024,
        Duration::from_secs(2),
    ));
    let out = compile_with_provider(&req, &Provider::new(proposal))
        .await
        .unwrap();
    assert!(
        out.questions.iter().any(|q| q.key == "const.refund_policy"),
        "{out:#?}"
    );
    assert!(
        !out.questions
            .iter()
            .any(|q| q.key == "intent.clarification")
    );
}

#[tokio::test]
async fn automatic_classification_wording_with_effect_none_is_not_a_veto() {
    let intent = "Look up the customer, classify the ticket automatically, and draft a reply.";
    let proposal = json!({"steps":[{"operation":"lookup","evidence":"Look up the customer"},{"operation":"classify","evidence":"classify the ticket automatically"},{"operation":"draft","evidence":"draft a reply"}],"effect":"none","effect_evidence":"","unknowns":[]});
    let req = CompileRequest::create(intent).with_authoring_policy(AuthoringPolicy::new(
        "mock/authoring",
        1024,
        Duration::from_secs(2),
    ));
    let out = compile_with_provider(&req, &Provider::new(proposal))
        .await
        .unwrap();
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "const.customer_directory"),
        "{out:#?}"
    );
    assert!(
        !out.questions
            .iter()
            .any(|q| q.key == "intent.clarification")
    );
}
