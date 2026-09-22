// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Explicit cognition contracts: HOT reads alone, WARM asks a bounded seat, COLD asks
//! one generative provider. All seats are injected hermetic doubles.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, Cognition, CompileRequest, CompileStatus, HotPolicy, NoProvider, Strategy,
    compile_with_cognition, compile_with_provider,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat, NONE_OPTION},
    outcome_document,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
const INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";
fn plan() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
}
fn request() -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(policy())
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

/// A proposal that reads the very words of a safeguard as an operation (« dédoublonne le
/// callback par identifiant » as a classify, « vérifie de nouveau la version courante … »
/// as a validate) invents nothing: the obligation the reader states carries those words,
/// and the doubled steps are not assembled. Measured on the base (E04C, gpt-5-mini): a
/// READY with classify and validate over a request that asks for a lookup and two safeguards.
#[tokio::test]
async fn a_step_over_the_words_of_a_safeguard_is_the_safeguard_never_a_second_operation() {
    let intent = "Quand le bouton Slack de validation est utilisé, retrouve le dossier dans MongoDB, dédoublonne le callback par identifiant et vérifie de nouveau la version courante du dossier avant l’action finale. Arrête-toi après ces étapes ; aucune autre action n’est demandée.";
    let dedup = "dédoublonne le callback par identifiant";
    let recheck = "vérifie de nouveau la version courante du dossier avant l'action finale";
    let proposal = json!({
        "steps": [
            {"op": "lookup", "detail": "le dossier dans MongoDB", "evidence": "retrouve le dossier dans MongoDB"},
            {"op": "classify", "detail": "dédoublonne le callback", "evidence": dedup},
            {"op": "validate", "detail": "la version courante du dossier", "evidence": recheck}
        ],
        "effects": [],
        "obligations": [
            {"kind": "dedup", "evidence": dedup},
            {"kind": "revision_check", "evidence": recheck}
        ],
        "constraints": [], "unknowns": []
    });
    let req = CompileRequest::create(intent).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &Provider::new(proposal))
        .await
        .unwrap();
    let doc = outcome_document(&out);
    let ops: Vec<&str> = doc["provenance"]["plan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["op"].as_str())
        .collect();
    assert_eq!(ops, ["lookup"], "{doc:#}");
    let kinds: Vec<&str> = doc["provenance"]["plan"]["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"dedup") && kinds.contains(&"revision_check"),
        "{doc:#}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(!keys(&out).contains(&"model"), "no language step: {out:#?}");
}

/// A seat that stops at its output cap returns a truncated text: the finding names the cap
/// and the tokens spent (the operator's knob), never a malformed shape.
#[tokio::test]
async fn a_seat_cut_at_its_output_cap_is_named_as_such() {
    struct Capped;
    impl ProviderInferDyn for Capped {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            Ok(InferResponse::new(
                vec![ContentBlock::Text {
                    text: "{\"steps\":[{\"op\":\"lookup\",\"detail\":\"le cli".to_owned(),
                }],
                TokenUsage::new(1900, 4000),
                StopReason::MaxTokens,
            ))
        }
    }
    let out = compile_with_provider(&request(), &Capped).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none());
    let finding = out
        .diagnostics
        .iter()
        .find(|d| d.target == "authoring_provider")
        .expect("an authoring_provider finding names the cap");
    assert!(
        finding.message.contains("4000 output tokens"),
        "{finding:?}"
    );
    assert!(
        finding.message.contains("--authoring-max-tokens"),
        "{finding:?}"
    );
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("one complete bounded JSON text")),
        "{out:#?}"
    );
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
    let out = nika_compile::compile(&CompileRequest::create(intent)).unwrap();
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
    let ready = nika_compile::compile(
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
    let out = nika_compile::compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let ready =
        nika_compile::compile(&CompileRequest::create(intent).answer("model", r#""mock/echo""#))
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
    let out =
        nika_compile::compile(&CompileRequest::create(forbidden).answer("model", r#""mock/echo""#))
            .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(
        doc["provenance"]["plan"]["effects"][0]["policy"],
        "forbidden"
    );
    assert!(!out.candidate.as_deref().unwrap().contains("nika:fetch"));
    let undecided = "Extrais les coordonnées de chaque candidature et prépare un accusé de réception. Je n'ai pas encore décidé si le workflow doit envoyer une invitation. Pose-moi la question avant de choisir.";
    let out = nika_compile::compile(&CompileRequest::create(undecided)).unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(keys(&out).contains(&"effect.send.include"), "{out:#?}");
    let conflict = "Extrais les coordonnées de chaque candidature et prépare un accusé de réception. Envoie ensuite une invitation. Il est aussi absolument interdit d'envoyer une invitation. Ces deux consignes doivent rester visibles comme une contradiction à résoudre.";
    let out = nika_compile::compile(&CompileRequest::create(conflict)).unwrap();
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

/// A lookup by identifier over a file the request already reads binds THAT file: the read
/// clause locates the material (« Read ./tickets.json, find ticket 42 »), so the only value
/// still open is the field that holds the identifier — never a second « directory ». The
/// located file is read once, by the lookup, and the write carries the selected record.
#[tokio::test]
async fn warm_lookup_over_a_located_read_binds_the_read_file_and_asks_only_the_id_field() {
    let intent = "Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json";
    let seat = Seat {
        choice: "lookup",
        asked: Mutex::new(Vec::new()),
    };
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: Some(&seat),
    };
    let out = compile_with_cognition(&CompileRequest::create(intent), cognition)
        .await
        .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Warm), "{out:#?}");
    assert_eq!(keys(&out), vec!["const.ticket_id_field"], "{out:#?}");
    let ready = compile_with_cognition(
        &CompileRequest::create(intent).answer("const.ticket_id_field", r#""id""#),
        cognition,
    )
    .await
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    let candidate = ready.candidate.as_deref().unwrap();
    assert!(candidate.contains("./tickets.json"), "{candidate}");
    assert!(candidate.contains("./ticket-42.json"), "{candidate}");
    assert!(candidate.contains("lookup_record"), "{candidate}");
    assert!(
        !candidate.contains("read_source"),
        "the located file is read once, by the lookup: {candidate}"
    );
    assert!(
        candidate.contains("read:\n    - ./tickets.json"),
        "the located file is the one read permit: {candidate}"
    );
    assert!(
        candidate.contains("ticket_id: '42'") && candidate.contains("ticket_id_field: id"),
        "{candidate}"
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

// ── The composer: a finite candidate set, deterministic feasibility, one seat call at most ──
//
// The coordinated drafting sentence is not explicit under the strict contract and forces
// COLD; "Fetch <url>" is an explicit literal step of the deterministic reading, so a
// proposal that drops it drops a recognized operation AND the only URL literal of the
// request.
const FETCH_INTENT: &str =
    "Fetch https://example.com/pricing. Harmonize the tone and prepare a summary.";

fn fetch_plan() -> Value {
    json!({"steps":[
        {"op":"fetch","detail":"https://example.com/pricing","evidence":"Fetch https://example.com/pricing"},
        {"op":"draft","detail":"the tone","evidence":"Harmonize the tone"},
        {"op":"draft","detail":"a summary","evidence":"prepare a summary"}],
        "effects":[],"obligations":[],"constraints":[],"unknowns":[]})
}
/// Drops the fetch of the literal URL: infeasible, never offered.
fn dropped_fetch_plan() -> Value {
    let mut p = fetch_plan();
    p["steps"].as_array_mut().unwrap().remove(0);
    p
}
/// Reads "prepare a summary" as classification too: a distinct admissible reading.
fn classified_fetch_plan() -> Value {
    let mut p = fetch_plan();
    p["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"op":"classify","detail":"the page","evidence":"prepare a summary"}));
    p
}
fn rotating(plans: &[Value]) -> Rotating {
    Rotating {
        plans: plans.iter().map(Value::to_string).collect(),
        calls: AtomicU32::new(0),
    }
}
fn candidates(doc: &Value) -> Vec<Value> {
    doc["provenance"]["decision"]["candidates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}
fn route(doc: &Value) -> String {
    doc["provenance"]["decision"]["route"].to_string()
}

#[tokio::test]
async fn compose_records_every_distinct_candidate_and_the_seat_picks_among_feasible_ones() {
    // (a) two distinct admissible plans, a seat choosing the second: both recorded feasible,
    // the chosen one assembled.
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
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    let listed = candidates(&doc);
    assert_eq!(listed.len(), 2, "{doc:#}");
    for (k, candidate) in listed.iter().enumerate() {
        assert_eq!(candidate["index"], k, "{candidate}");
        assert_eq!(candidate["feasible"], true, "{candidate}");
        assert_eq!(candidate["reasons"], json!([]), "{candidate}");
        assert_eq!(candidate["source"]["kind"], "cold_sample", "{candidate}");
        assert!(candidate["signature"].is_array(), "{candidate}");
        assert!(candidate["plan"]["operations"].is_array(), "{candidate}");
    }
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 1);
    assert!(
        listed[1]["plan"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "compute"),
        "the chosen candidate is the one with the code rule: {doc:#}"
    );
    assert!(route(&doc).contains("compose: seat"), "{}", route(&doc));
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    // The seat saw exactly the feasible candidates plus NONE, each described by its
    // signature and what differs.
    let asked = seat.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].keys(), ["plan-0", "plan-1", NONE_OPTION]);
    let with_compute = asked[0].options.iter().find(|o| o.key == "plan-1").unwrap();
    assert!(
        with_compute.description.contains("op:compute"),
        "{}",
        with_compute.description
    );
}

#[tokio::test]
async fn compose_never_offers_an_infeasible_candidate_to_the_seat() {
    // (b) the sample that drops the fetch of the literal URL is recorded infeasible with
    // its reasons and the seat receives only the two feasible readings.
    let provider = rotating(&[fetch_plan(), dropped_fetch_plan(), classified_fetch_plan()]);
    let seat = ChoosePlan {
        choice: "plan-0",
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    let doc = outcome_document(&out);
    let listed = candidates(&doc);
    assert_eq!(listed.len(), 3, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    let infeasible = &listed[1];
    assert_eq!(infeasible["feasible"], false, "{infeasible}");
    assert_eq!(infeasible["source"]["sample"], 1);
    let reasons = infeasible["reasons"].to_string();
    assert!(
        reasons.contains("dropped the recognized operation `fetch`"),
        "{reasons}"
    );
    assert!(reasons.contains("https://example.com/pricing"), "{reasons}");
    let asked = seat.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].keys(), ["plan-0", "plan-1", NONE_OPTION]);
    for option in &asked[0].options {
        assert!(
            option.key == NONE_OPTION || option.description.contains("op:fetch"),
            "an option without the fetch was offered: {option:?}"
        );
    }
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 0);
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    // The URL is bound from the request: only the runtime model is asked.
    assert_eq!(keys(&out), ["model"], "{out:#?}");
}

#[tokio::test]
async fn compose_with_a_single_feasible_candidate_never_calls_the_seat() {
    // (c) one feasible reading beside an infeasible one: no seat call, route "single".
    let provider = rotating(&[fetch_plan(), dropped_fetch_plan()]);
    let seat = ChoosePlan {
        choice: NONE_OPTION,
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(2));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert!(seat.asked.lock().unwrap().is_empty(), "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(candidates(&doc).len(), 2, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 1);
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 0);
    assert!(route(&doc).contains("compose: single"), "{}", route(&doc));
    assert!(
        doc["provenance"]["decision"]
            .get("warm_after_cold")
            .is_none(),
        "{doc:#}"
    );
    assert_eq!(keys(&out), ["model"], "{out:#?}");
}

#[tokio::test]
async fn compose_seat_none_is_a_clarification_with_the_candidates_on_record() {
    // (d) the seat finds none faithful: nothing assembled, the candidates stay recorded.
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
    let doc = outcome_document(&out);
    assert_eq!(candidates(&doc).len(), 2, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    assert!(doc["provenance"]["decision"]["selected_candidate"].is_null());
    assert_eq!(
        doc["provenance"]["decision"]["warm_after_cold"]["choice"],
        NONE_OPTION
    );
    assert!(
        route(&doc).contains("compose: seat none"),
        "{}",
        route(&doc)
    );
}

#[tokio::test]
async fn compose_ranks_deterministically_without_a_seat_and_asks_when_nothing_is_feasible() {
    // Several feasible candidates and no seat: the documented scorer keeps the candidate
    // closest to every accepted sample (support-weighted medoid); a tie keeps the first.
    let provider = rotating(&[fetch_plan(), classified_fetch_plan(), fetch_plan()]);
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 0);
    assert!(
        route(&doc).contains("compose: deterministic rank"),
        "{}",
        route(&doc)
    );
    // Zero feasible candidates: the reasons are the findings and a human settles it.
    let provider = rotating(&[dropped_fetch_plan()]);
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(2));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(out.candidate.is_none());
    assert!(out.provenance.strategy.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .contains("dropped the recognized operation `fetch`")),
        "{out:#?}"
    );
    let doc = outcome_document(&out);
    assert_eq!(candidates(&doc).len(), 1, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 0);
    assert!(doc["provenance"]["decision"]["selected_candidate"].is_null());
    assert!(
        route(&doc).contains("compose: none feasible"),
        "{}",
        route(&doc)
    );
}

#[tokio::test]
async fn compose_records_pattern_dimensions_without_composing_an_inexpressible_variant() {
    // The recalled candidates may suggest fan-out or fan-in; the assembler expresses one
    // linear chain today, so the dimension is recorded and no variant is composed.
    let provider = Provider::new(plan());
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    let doc = outcome_document(&out);
    let compose = &doc["provenance"]["decision"]["compose"];
    assert_eq!(compose["cap"], 8, "{doc:#}");
    let dimensions = compose["pattern_dimensions"].as_array().unwrap();
    for dimension in dimensions {
        assert!(
            matches!(dimension["dimension"].as_str(), Some("fanout" | "fanin")),
            "{dimension}"
        );
        assert!(
            !dimension["hits"].as_array().unwrap().is_empty(),
            "{dimension}"
        );
        assert_eq!(dimension["expressible"], false, "{dimension}");
        assert!(dimension["fixed_by_request"].is_boolean(), "{dimension}");
    }
    assert!(
        candidates(&doc)
            .iter()
            .all(|c| c["source"]["kind"] == "cold_sample"),
        "{doc:#}"
    );
}

/// A proposal may wrap a line or drop a double space in its evidence; the compiler anchors
/// it to the exact request excerpt instead of rejecting the whole plan. A changed word is
/// still no excerpt at all.
#[tokio::test]
async fn whitespace_folded_evidence_is_anchored_to_the_exact_excerpt() {
    let mut folded = plan();
    folded["steps"][0]["evidence"] = json!("consulte   le\nclient");
    let provider = Provider::new(folded);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    let doc = outcome_document(&out);
    let evidence: Vec<&str> = doc["provenance"]["plan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["evidence"].as_str())
        .collect();
    assert!(evidence.contains(&"consulte le client"), "{evidence:?}");
    let mut altered = plan();
    altered["steps"][0]["evidence"] = json!("consulte la cliente");
    let provider = Provider::new(altered);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_none_or(|p| p["operations"].as_array().is_none_or(Vec::is_empty)),
        "{out:#?}"
    );
}

/// A provider's strict structured-output mode answers `null` for optional properties
/// (categories, the bypass evidence); the plan is still decoded and assembled.
#[tokio::test]
async fn explicit_nulls_from_a_strict_provider_are_absent_fields() {
    let mut with_nulls = plan();
    with_nulls["steps"][0]["categories"] = Value::Null;
    with_nulls["steps"][1]["categories"] = Value::Null;
    with_nulls["approval_bypass"] = json!({"present": false, "evidence": null});
    with_nulls["obligations"] = json!([]);
    let provider = Provider::new(with_nulls);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !d.message.contains("not a valid closed semantic plan")),
        "{out:#?}"
    );
    assert!(out.provenance.plan.is_some(), "{out:#?}");
}

/// The second clean-shell gate: a proposal that kept "write a brief of at most 5 bullet
/// points … to ./out/brief.md" as a write and produced no draft was assembled as a copy of
/// the fetched page. Such a candidate is not feasible; the compiler asks instead.
#[tokio::test]
async fn a_proposal_that_keeps_the_write_and_drops_the_draft_is_not_feasible() {
    let intent = "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt and write a brief of at most 5 bullet points to ./out/brief.md explaining what the document specifies.";
    let proposal = json!({
        "steps": [{"op": "fetch", "detail": "https://www.rfc-editor.org/rfc/rfc2324.txt", "evidence": "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt"}],
        "effects": [{"verb": "write", "target": "./out/brief.md", "policy": "automatic", "evidence": "write a brief of at most 5 bullet points to ./out/brief.md explaining what the document specifies"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [{"text": "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt", "role": "operation"}, {"text": "write a brief of at most 5 bullet points to ./out/brief.md explaining what the document specifies.", "role": "effect"}],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("names content no step produces")),
        "{out:#?}"
    );
}

/// The reader's refund guard is a word-level backstop; the proposal's accounting settles it.
#[tokio::test]
async fn the_refund_backstop_yields_to_the_proposals_accounting() {
    // A status value in a filter, read as an operation region: no refund effect is missing.
    let intent = "Read ./data/orders.csv, keep only the rows whose status is refunded and whose amount_eur is greater than 100, and write those rows to ./out/kept.csv with the same header.";
    let proposal = json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv"},
            {"op": "compute", "detail": "rows whose status is refunded and whose amount_eur is greater than 100", "evidence": "keep only the rows whose status is refunded and whose amount_eur is greater than 100"}
        ],
        "effects": [{"verb": "write", "target": "./out/kept.csv", "policy": "automatic", "evidence": "write those rows to ./out/kept.csv with the same header"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./data/orders.csv,", "role": "operation"},
            {"text": "keep only the rows whose status is refunded and whose amount_eur is greater than 100,", "role": "operation"},
            {"text": "and write those rows to ./out/kept.csv with the same header.", "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !d.message.contains("mentions a refund")),
        "{out:#?}"
    );
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_some_and(|p| p["unknowns"].as_array().is_some_and(Vec::is_empty)),
        "{out:#?}"
    );
    // A refund effect the proposal carries satisfies the guard outright.
    let intent = "Look up the customer in ./crm/customers.json, draft a polite reply, and ask a human to approve before the refund is posted.";
    let proposal = json!({
        "steps": [
            {"op": "lookup", "detail": "the customer", "evidence": "Look up the customer in ./crm/customers.json"},
            {"op": "draft", "detail": "a polite reply", "evidence": "draft a polite reply"}
        ],
        "effects": [{"verb": "refund", "target": "the refund", "policy": "human_first", "evidence": "ask a human to approve before the refund is posted"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Look up the customer in ./crm/customers.json,", "role": "operation"},
            {"text": "draft a polite reply,", "role": "operation"},
            {"text": "and ask a human to approve before the refund is posted.", "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !d.message.contains("mentions a refund")),
        "{out:#?}"
    );
}

/// A proposal may abbreviate a long clause with an ellipsis ("Sépare-les en trois fichiers :
/// ./out/a.json ... ./out/c.json"); the excerpt is the contiguous request span from the first
/// fragment to the last. Fragments out of order or absent are still no excerpt.
#[tokio::test]
async fn an_ellipsis_in_the_evidence_names_the_span_it_abbreviates() {
    let mut abbreviated = plan();
    abbreviated["steps"][0]["evidence"] = json!("consulte ... client");
    let provider = Provider::new(abbreviated);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    let doc = outcome_document(&out);
    let evidence: Vec<&str> = doc["provenance"]["plan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["evidence"].as_str())
        .collect();
    assert!(evidence.contains(&"consulte le client"), "{evidence:?}");
    let mut reversed = plan();
    reversed["steps"][0]["evidence"] = json!("client ... consulte");
    let provider = Provider::new(reversed);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("lacks an exact source excerpt")),
        "{out:#?}"
    );
}

/// The fourth gate: a Spanish inventory rule ("una línea por cada producto cuyo stock sea menor
/// que su minimo … Total urgentes: N") was read as a constraint region and produced nothing;
/// the workflow copied the CSV and reported ready. A producing region without an element is
/// a dropped clause: the compiler asks instead.
#[tokio::test]
async fn a_region_the_proposal_read_but_produced_nothing_for_is_not_understood() {
    let intent = "Lee ./data/inventario.csv y escribe ./out/alerta.txt con una línea por cada producto cuyo stock sea menor que su minimo.";
    let dropped = json!({
        "steps": [{"op": "read", "detail": "./data/inventario.csv", "evidence": "Lee ./data/inventario.csv"}],
        "effects": [{"verb": "write", "target": "./out/alerta.txt", "policy": "automatic", "evidence": "escribe ./out/alerta.txt"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Lee ./data/inventario.csv", "role": "operation"},
            {"text": "y escribe ./out/alerta.txt", "role": "effect"},
            {"text": "con una línea por cada producto cuyo stock sea menor que su minimo.", "role": "constraint"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(dropped);
    let req = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("produced nothing for it")),
        "{out:#?}"
    );
}

/// A prohibition proposed as a computation ("Do not copy more than 10 consecutive words")
/// becomes a constraint that shapes the prompt; no rule question is asked for it.
#[tokio::test]
async fn a_prohibition_proposed_as_a_computation_is_a_constraint() {
    let intent = "Fetch https://example.com/spec and draft ./out/brief.md with 4 bullets in your own words. Do not copy more than 10 consecutive words from the page.";
    let proposal = json!({
        "steps": [
            {"op": "fetch", "detail": "https://example.com/spec", "evidence": "Fetch https://example.com/spec"},
            {"op": "draft", "detail": "./out/brief.md with 4 bullets in your own words", "evidence": "draft ./out/brief.md with 4 bullets in your own words"},
            {"op": "compute", "detail": "more than 10 consecutive words", "evidence": "Do not copy more than 10 consecutive words from the page"}
        ],
        "effects": [{"verb": "write", "target": "./out/brief.md", "policy": "automatic", "evidence": "draft ./out/brief.md with 4 bullets in your own words"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Fetch https://example.com/spec", "role": "operation"},
            {"text": "and draft ./out/brief.md with 4 bullets in your own words.", "role": "operation"},
            {"text": "Do not copy more than 10 consecutive words from the page.", "role": "constraint"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    assert!(
        plan["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["op"] != "compute"),
        "{plan:#}"
    );
    assert!(
        plan["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap_or("").starts_with("Do not copy")),
        "{plan:#}"
    );
}
