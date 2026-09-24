// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Explicit cognition contracts: HOT reads alone, WARM asks a bounded seat, COLD asks
//! one generative provider. All seats are injected hermetic doubles.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileRequest, CompileStatus, Strategy, outcome_document};
use nika_compile_cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_provider,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat, NONE_OPTION},
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};

mod common;
use common::{INTENT, Provider, keys, plan, policy, request};
use std::{
    sync::{Mutex, atomic::Ordering},
    time::Duration,
};

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
    // Each case names the calls it costs: one, or two when the defect is an evidence the
    // request never wrote (the one bounded repair call, answered here with the same plan).
    let mut cases = vec![(json!({"yaml":"nika: invented"}), 1)];
    // A model may not weaken, lift or settle the deterministic policy of a recognized effect.
    for policy in ["automatic", "forbidden", "conflict", "unspecified"] {
        let mut p = plan();
        p["effects"][0]["policy"] = json!(policy);
        cases.push((p, 1));
    }
    // Invented effects need an exact excerpt the request never wrote.
    for verb in ["send", "publish"] {
        let mut p = plan();
        p["effects"].as_array_mut().unwrap().push(json!({"verb":verb,"target":"la réponse","policy":"automatic","evidence":"envoie la réponse"}));
        cases.push((p, 2));
    }
    for (field, calls) in [("op", 1), ("evidence", 2)] {
        let mut p = plan();
        p["steps"][0][field] = json!("invented");
        cases.push((p, calls));
    }
    let mut p = plan();
    p["unknowns"] = json!(["Use a previous approval for a different amount"]);
    cases.push((p, 1));
    let mut p = plan();
    p["effects"][0]["evidence"] = json!("unmentioned refund authority");
    cases.push((p, 2));
    for (p, calls) in cases {
        let provider = Provider::new(p.clone());
        let out = compile_with_provider(&request(), &provider).await.unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{p}");
        assert!(out.candidate.is_none(), "{p}");
        assert!(
            !keys(&out).contains(&"const.refund_policy"),
            "{p}: {out:#?}"
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), calls, "{p}");
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
