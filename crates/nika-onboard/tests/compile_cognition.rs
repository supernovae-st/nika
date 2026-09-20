// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Explicit cognition contracts: HOT reads alone, WARM asks a bounded seat, COLD asks
//! one generative provider. All seats are injected hermetic doubles.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileRequest, CompileStatus, HotPolicy, NoProvider, Strategy,
    compile_with_cognition, compile_with_provider,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat, NONE_OPTION},
    outcome_document,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
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
/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
const INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";
fn plan() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
}
fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
}
fn request() -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(policy())
}
fn keys(out: &nika_onboard::compile::CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

#[tokio::test]
async fn provider_opt_in_returns_real_questions_and_versioned_usage() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold));
    let doc = outcome_document(&out);
    assert_eq!(doc["compile_version"], 2);
    assert_eq!(doc["provenance"]["cognition"], "explicitProvider");
    assert_eq!(doc["provenance"]["strategy"], "cold");
    assert_eq!(doc["provenance"]["authoring"]["calls"], 1);
    assert_eq!(doc["provenance"]["authoring"]["input_tokens"], 120);
    assert_eq!(
        doc["provenance"]["plan"]["effects"][0]["policy"],
        "human_first"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let out = compile_with_provider(
        &request()
            .answer("model", r#""mock/echo""#)
            .answer("const.customer_directory", r#""customers.json""#)
            .answer(
                "const.refund_policy",
                r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
            )
            .answer(
                "const.refund_endpoint",
                r#""https://refund.example.invalid/refunds""#,
            ),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.check_preview.unwrap().report.is_clean());
    let source = out.candidate.unwrap();
    let doc: Value = serde_yaml_bw::from_str(&source).unwrap();
    assert_eq!(doc["tasks"]["refund"]["invoke"]["args"]["method"], "POST");
    assert_eq!(
        doc["tasks"]["refund"]["when"],
        "${{ with.approved == true }}"
    );
    assert!(doc["tasks"]["refund_review"]["invoke"]["tool"] == "nika:prompt");
    assert_eq!(
        doc["permits"]["net"]["http"],
        json!(["refund.example.invalid"])
    );
}

#[tokio::test]
async fn no_opt_in_and_exact_skeleton_never_call_provider_or_change_v1() {
    let provider = Provider::new(plan());
    for req in [
        CompileRequest::create(INTENT),
        CompileRequest::create("hello").with_authoring_policy(policy()),
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
    // A model may not weaken, lift or settle the deterministic policy of a recognized effect.
    for policy in ["automatic", "forbidden", "conflict", "unspecified"] {
        let mut p = plan();
        p["effects"][0]["policy"] = json!(policy);
        cases.push(p);
    }
    // Invented effects need an exact excerpt the request never wrote.
    for verb in ["send", "publish"] {
        let mut p = plan();
        p["effects"].as_array_mut().unwrap().push(json!({"verb":verb,"target":"la réponse","policy":"automatic","evidence":"envoie la réponse"}));
        cases.push(p);
    }
    for field in ["op", "evidence"] {
        let mut p = plan();
        p["steps"][0][field] = json!("invented");
        cases.push(p);
    }
    let mut p = plan();
    p["unknowns"] = json!(["Use a previous approval for a different amount"]);
    cases.push(p);
    let mut p = plan();
    p["effects"][0]["evidence"] = json!("unmentioned refund authority");
    cases.push(p);
    for p in cases {
        let provider = Provider::new(p.clone());
        let out = compile_with_provider(&request(), &provider).await.unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{p}");
        assert!(out.candidate.is_none(), "{p}");
        assert!(
            !keys(&out).contains(&"const.refund_policy"),
            "{p}: {out:#?}"
        );
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
    assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
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
    // The deterministic reader recognized the gated refund: a proposal omitting it
    // cannot drop it, and the human-first policy survives.
    let mut omitted = plan();
    omitted["effects"] = json!([]);
    let out = compile_with_provider(&request(), &Provider::new(omitted))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    // An approval bypass is never compiled, whatever the model says.
    let intent = format!("{INTENT} Rembourse automatiquement sans mon accord.");
    let req = CompileRequest::create(intent).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
}

#[tokio::test]
async fn recognized_send_is_a_bound_effect_never_a_draft_only_candidate() {
    let intent =
        "Consulte le client, classe le problème, prépare une réponse et envoyer la réponse.";
    let out = nika_onboard::compile::compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"const.send_endpoint"), "{out:#?}");
    assert!(!keys(&out).contains(&"intent.clarification"));
    let doc = outcome_document(&out);
    assert_eq!(doc["provenance"]["plan"]["effects"][0]["verb"], "send");
    assert_eq!(
        doc["provenance"]["plan"]["effects"][0]["policy"],
        "automatic"
    );
    let ready = nika_onboard::compile::compile(
        &CompileRequest::create(intent)
            .answer("model", r#""mock/echo""#)
            .answer("const.customer_directory", r#""customers.json""#)
            .answer(
                "const.send_endpoint",
                r#""https://mail.example.invalid/send""#,
            ),
    )
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    let source = ready.candidate.unwrap();
    let doc: Value = serde_yaml_bw::from_str(&source).unwrap();
    assert_eq!(doc["tasks"]["send"]["invoke"]["args"]["method"], "POST");
    assert!(
        doc["tasks"].get("send_review").is_none(),
        "an automatic send has no invented gate"
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
        let proposal = json!({"steps":[{"op":"lookup","detail":lookup,"evidence":lookup},{"op":"classify","detail":classify,"evidence":classify},{"op":"draft","detail":draft,"evidence":draft}],"effects":[{"verb":"refund","target":effect,"policy":"human_first","evidence":effect}],"obligations":[],"constraints":[],"unknowns":[]});
        let req = CompileRequest::create(intent).with_authoring_policy(policy());
        let out = compile_with_provider(&req, &Provider::new(proposal))
            .await
            .unwrap();
        assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
        assert!(!keys(&out).contains(&"intent.clarification"));
        assert_eq!(out.provenance.strategy, Some(Strategy::Cold));
    }
}

#[tokio::test]
async fn complete_explicit_replacement_resolves_old_automatic_refund_request() {
    let req = CompileRequest::create(format!("{INTENT} Refund automatically."))
        .with_authoring_policy(policy())
        .answer(
            "intent.clarification",
            serde_json::to_string(INTENT).unwrap(),
        );
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
    assert!(!keys(&out).contains(&"intent.clarification"));
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
    assert!(!keys(&out).contains(&"const.refund_policy"));
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
        let proposal = json!({"steps":[{"op":"lookup","detail":"the customer","evidence":lookup},{"op":"draft","detail":"a reply","evidence":draft}],"effects":[],"obligations":[],"constraints":[],"unknowns":[]});
        let req = CompileRequest::create(intent).with_authoring_policy(policy());
        let out = compile_with_provider(&req, &Provider::new(proposal))
            .await
            .unwrap();
        assert!(out.candidate.is_none(), "{intent}");
        assert!(
            keys(&out).contains(&"intent.clarification"),
            "{intent}: {out:#?}"
        );
        assert!(
            !keys(&out).contains(&"const.customer_directory"),
            "{intent}: a bypassed authority must not become a lookup+draft candidate"
        );
    }
}

#[tokio::test]
async fn french_fichier_is_not_the_yesterday_bypass_phrase() {
    let intent = "Consulte le fichier client, classe le problème, prépare une réponse. Demande un accord humain avant le remboursement.";
    let req = CompileRequest::create(intent).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
    assert!(!keys(&out).contains(&"intent.clarification"));
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
}

#[tokio::test]
async fn automatic_classification_wording_with_effect_none_is_not_a_veto() {
    let intent = "Look up the customer, classify the ticket automatically, and draft a reply.";
    let req = CompileRequest::create(intent).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &Provider::new(plan()))
        .await
        .unwrap();
    assert!(keys(&out).contains(&"const.customer_directory"), "{out:#?}");
    assert!(!keys(&out).contains(&"intent.clarification"));
}

#[tokio::test]
async fn exact_support_clauses_never_reach_an_opted_in_provider() {
    let provider = Provider::new(plan());
    let req = CompileRequest::create(
        "Route support tickets, look up the customer, draft a reply, and ask me before any refund",
    )
    .with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert!(keys(&out).contains(&"const.refund_policy"), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(doc["compile_version"], 1);
    assert_eq!(doc["provenance"]["cognition"], "deterministicOnly");
    assert_eq!(doc["provenance"]["strategy"], "support");
    assert!(doc["provenance"].get("authoring").is_none());
}

// ── HOT receipts beyond support ────────────────────────────────────────────
#[test]
fn hot_reads_a_file_transforms_it_and_writes_the_result_with_zero_calls() {
    let intent = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";
    let out = nika_onboard::compile::compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let ready = nika_onboard::compile::compile(
        &CompileRequest::create(intent).answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    assert!(ready.check_preview.as_ref().unwrap().report.is_clean());
    let doc: Value = serde_yaml_bw::from_str(ready.candidate.as_deref().unwrap()).unwrap();
    assert_eq!(doc["const"]["source_path"], "./notes/brief.md");
    assert_eq!(doc["const"]["output_path"], "./out/summary.md");
    assert_eq!(doc["permits"]["fs"]["read"], json!(["./notes/brief.md"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["./out/summary.md"]));
    assert!(doc["tasks"]["read_source"]["invoke"]["tool"] == "nika:read");
    assert!(doc["tasks"]["draft"]["infer"].is_object());
    assert!(doc["tasks"]["write_output"]["invoke"]["tool"] == "nika:write");
}

#[test]
fn hot_prohibition_and_indecision_are_honoured_without_a_model() {
    let forbidden = "Extrais les coordonnées de chaque candidature et prépare un accusé de réception. Il est absolument interdit d'envoyer une invitation.";
    let out = nika_onboard::compile::compile(
        &CompileRequest::create(forbidden).answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(
        doc["provenance"]["plan"]["effects"][0]["policy"],
        "forbidden"
    );
    assert!(!out.candidate.as_deref().unwrap().contains("nika:fetch"));
    let undecided = "Extrais les coordonnées de chaque candidature et prépare un accusé de réception. Je n'ai pas encore décidé si le workflow doit envoyer une invitation. Pose-moi la question avant de choisir.";
    let out = nika_onboard::compile::compile(&CompileRequest::create(undecided)).unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(keys(&out).contains(&"effect.send.include"), "{out:#?}");
    let conflict = "Extrais les coordonnées de chaque candidature et prépare un accusé de réception. Envoie ensuite une invitation. Il est aussi absolument interdit d'envoyer une invitation. Ces deux consignes doivent rester visibles comme une contradiction à résoudre.";
    let out = nika_onboard::compile::compile(&CompileRequest::create(conflict)).unwrap();
    assert_eq!(out.status, CompileStatus::Refused, "{out:#?}");
    assert!(out.candidate.is_none());
}

// ── WARM: a finite ambiguity settled by an injected seat ───────────────────
struct Seat {
    choice: &'static str,
    asked: Mutex<Vec<ChoiceQuestion>>,
}
impl DecisionSeat for Seat {
    fn name(&self) -> &'static str {
        "double/seat"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            let mut answer = ChoiceAnswer::new(self.choice, "double-1.0");
            answer.probabilities.insert(self.choice.to_owned(), 0.9);
            answer.confidence = Some(0.8);
            answer.input_tokens = Some(40);
            answer.output_tokens = Some(2);
            Ok(answer)
        })
    }
}
// "cherche les éléments demandés" names no corpus and no record source: a genuine finite ambiguity.
const WARM: &str =
    "Cherche les éléments demandés, puis propose par écrit trois créneaux compatibles.";

#[tokio::test]
async fn warm_settles_a_finite_ambiguity_through_the_seat_and_records_it() {
    let seat = Seat {
        choice: "lookup",
        asked: Mutex::new(Vec::new()),
    };
    let out = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(WARM),
        Cognition {
            provider: None,
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Warm), "{out:#?}");
    let asked = seat.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].keys().contains(&NONE_OPTION.to_owned()));
    assert!(asked[0].keys().contains(&"search".to_owned()));
    assert!(asked[0].keys().contains(&"lookup".to_owned()));
    let doc = outcome_document(&out);
    assert_eq!(doc["provenance"]["cognition"], "explicitDecision");
    assert_eq!(doc["provenance"]["decision"]["seat"], "double/seat");
    assert_eq!(
        doc["provenance"]["decision"]["questions"][0]["choice"],
        "lookup"
    );
    assert!(
        keys(&out).iter().any(|k| k.ends_with("_directory")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn warm_none_never_picks_the_least_wrong_option() {
    let seat = Seat {
        choice: NONE_OPTION,
        asked: Mutex::new(Vec::new()),
    };
    let out = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(WARM),
        Cognition {
            provider: None,
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(out.provenance.strategy.is_none());
}

#[tokio::test]
async fn warm_seat_cannot_choose_outside_the_offered_options() {
    let seat = Seat {
        choice: "send",
        asked: Mutex::new(Vec::new()),
    };
    let out = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(WARM),
        Cognition {
            provider: None,
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let doc = outcome_document(&out);
    assert!(doc["provenance"]["decision"]["questions"][0]["error"].is_string());
}

#[tokio::test]
async fn fully_readable_intents_never_call_a_permitted_seat() {
    let seat = Seat {
        choice: "lookup",
        asked: Mutex::new(Vec::new()),
    };
    let provider = Provider::new(plan());
    let out = compile_with_cognition(
        &CompileRequest::create("Look up the customer, classify the ticket and draft a reply.")
            .with_authoring_policy(policy()),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot));
    assert!(seat.asked.lock().unwrap().is_empty());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

// ── COLD best-of-N: agreement, never a vote that hides a dropped effect ───────
struct Rotating {
    plans: Vec<String>,
    calls: AtomicU32,
}
impl ProviderInferDyn for Rotating {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
        let text = self.plans[index % self.plans.len()].clone();
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(100, 50),
            StopReason::EndTurn,
        ))
    }
}

#[tokio::test]
async fn cold_best_of_three_keeps_the_plan_the_others_agree_with() {
    // Sample 1 reads "classe le problème" as a code rule the request never asked; samples 2 and 3 agree.
    let mut invented = plan();
    invented["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"op":"compute","detail":"le problème","evidence":"classe le problème"}));
    let provider = Rotating {
        plans: vec![invented.to_string(), plan().to_string(), plan().to_string()],
        calls: AtomicU32::new(0),
    };
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
    let provider = Rotating {
        plans: vec![unanchored.to_string()],
        calls: AtomicU32::new(0),
    };
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
    let out = nika_onboard::compile::compile(&CompileRequest::create(SWALLOWED)).unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(out.provenance.strategy.is_none(), "{out:#?}");
    let doc = outcome_document(&out);
    let route = doc["provenance"]["decision"]["route"].to_string();
    assert!(route.contains("hot rejected"), "{route}");
    // The legacy contract (ablation only) would have admitted it: the false HOT we measure.
    let legacy = nika_onboard::compile::compile(
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
    let out = nika_onboard::compile::compile(&CompileRequest::create(
        "Fetch https://example.com/pricing and write the result to ./out/pricing.md",
    ))
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
}

#[test]
fn hot_policy_off_never_admits_prose() {
    let out = nika_onboard::compile::compile(
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
struct ChoosePlan {
    choice: &'static str,
    asked: Mutex<Vec<ChoiceQuestion>>,
}
impl DecisionSeat for ChoosePlan {
    fn name(&self) -> &'static str {
        "double/plans"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            Ok(ChoiceAnswer::new(self.choice, "double-1.0"))
        })
    }
}

fn disagreeing_provider() -> Rotating {
    let mut with_compute = plan();
    with_compute["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"op":"compute","detail":"le problème","evidence":"classe le problème"}));
    Rotating {
        plans: vec![
            plan().to_string(),
            with_compute.to_string(),
            plan().to_string(),
        ],
        calls: AtomicU32::new(0),
    }
}

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
