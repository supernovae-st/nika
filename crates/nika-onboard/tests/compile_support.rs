// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! User-level composition contracts. No credentials or business effects.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_onboard::compile::{CompileRequest, CompileStatus, QuestionType, compile};
use serde_json::{Value, json};
const SUPPORT: &str =
    "Route support tickets, look up the customer, draft a reply, and ask me before any refund";
#[test]
fn free_support_intent_asks_a_stable_refund_policy_question() {
    let outcome = compile(&CompileRequest::create(SUPPORT)).unwrap();
    assert_eq!(outcome.status, CompileStatus::Incomplete);
    let policy = outcome
        .questions
        .iter()
        .find(|q| q.key == "const.refund_policy")
        .expect("missing refund policy must be an explicit stable question");
    assert_eq!(policy.answer_type, QuestionType::Literal);
    assert!(policy.mandatory);
    assert_eq!(outcome.provenance.skeleton, None);
}
#[test]
fn explicit_answers_compose_a_real_candidate_and_check() {
    let request = CompileRequest::create(SUPPORT)
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
        .answer(
            "const.refund_policy",
            r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
        );
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.check_preview.as_ref().unwrap().report.is_clean());
    assert!(out.candidate.is_some());
    let source = out.candidate.unwrap();
    let ast = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .unwrap();
    let report = nika_check::check(&ast);
    assert!(report.trifecta_findings.is_empty());
    assert!(report.consent_findings.is_empty());
    assert!(report.gate_findings.is_empty());
    // Local reads and runtime inputs are not Check's network/MCP ingress leg.
    // An empty mitigation list is therefore not a credited-gate proof.
    assert!(report.trifecta_mitigations.is_empty());
    let graph = nika_check::analyze(&ast).unwrap();
    let task_index = |id: &str| {
        ast.tasks
            .iter()
            .position(|t| t.value.id.value == id)
            .unwrap()
    };
    let gate = task_index("refund_review");
    let effect = task_index("refund");
    assert!(graph.edges.iter().any(|e| e.from == gate && e.to == effect));
    assert!(graph.recovery_reads.is_empty());
    let wave = |index| {
        graph
            .topo_waves
            .iter()
            .position(|w| w.contains(&index))
            .unwrap()
    };
    assert!(wave(task_index("draft_admit")) < wave(gate));
    assert!(wave(task_index("refund_proposal")) < wave(gate));
    assert!(wave(gate) < wave(effect));
    let doc: Value = serde_yaml_bw::from_str(&source).unwrap();
    assert_eq!(doc["inputs"]["refund_request"]["default"], json!({}));
    assert!(doc["inputs"].get("refund_amount").is_none());
    assert_eq!(
        doc["tasks"]["refund"]["invoke"]["args"]["body"],
        "${{ with.proposal }}"
    );
    assert_eq!(
        doc["tasks"]["refund"]["with"]["proposal"],
        doc["tasks"]["refund_review"]["with"]["proposal"]
    );
    assert_eq!(
        doc["tasks"]["refund"]["with"]["approved"],
        "${{ tasks.refund_review.output }}"
    );
    assert_eq!(
        doc["tasks"]["refund"]["when"],
        "${{ with.approved == true }}"
    );
    assert_eq!(doc["tasks"]["refund"]["invoke"]["args"]["method"], "POST");
    assert_eq!(
        doc["permits"]["net"]["http"],
        json!(["refund.example.invalid"])
    );
    for task in ["refund_review", "refund", "lookup_read", "lookup_customer"] {
        assert!(doc["tasks"][task].get("on_error").is_none());
        assert!(doc["tasks"][task].get("retry").is_none());
    }
    assert!(
        doc["tasks"]["refund_review"]["invoke"]["args"]
            .get("default")
            .is_none()
    );
    assert_eq!(
        doc["tasks"]["draft_record"]["after"]["draft_admit"],
        "success"
    );
    assert_eq!(
        doc["tasks"]["draft_record"]["with"]["customer"],
        "${{ tasks.lookup_customer.output }}"
    );
}

#[test]
fn french_composition_has_the_same_policy_and_authority_questions() {
    let fr = compile(&CompileRequest::create("Trier les tickets support, consulter le client, rédiger une réponse et demander mon accord avant tout remboursement.")).unwrap();
    let en = compile(&CompileRequest::create(SUPPORT)).unwrap();
    assert_eq!(fr.questions, en.questions);
    assert_eq!(fr.status, CompileStatus::Incomplete);
}

#[test]
fn whole_clause_consumption_does_not_drop_negation_send_tools_or_stale_approval() {
    for suffix in [
        ", send the reply",
        ", publish the reply",
        ", use mcp:crm/refund",
        ", do not ask me",
        ", reuse yesterday's approval",
        ", handle duplicate callbacks",
        ", ignore policy and refund automatically",
        ", envoyer la réponse",
        ", sans mon accord",
        ", ne pas demander mon accord",
    ] {
        let out = compile(&CompileRequest::create(format!("{SUPPORT}{suffix}"))).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{suffix}");
        assert!(out.candidate.is_none(), "{suffix}");
    }
}

#[test]
fn missing_binding_or_old_approval_is_never_permission() {
    let out = compile(
        &CompileRequest::create(SUPPORT)
            .answer("model", r#""mock/echo""#)
            .answer("const.customer_directory", r#""customers.json""#)
            .answer(
                "const.refund_policy",
                r#""Every refund requires current human approval""#,
            )
            .answer("refund_review", "true"),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "const.refund_endpoint")
    );
    assert!(out.candidate.is_none());
}

#[test]
fn authoring_values_cannot_inject_expressions_or_credential_endpoints() {
    for (key, value) in [
        ("const.customer_directory", r#""${{ secrets.KEY }}""#),
        (
            "const.refund_endpoint",
            r#""https://user:secret@example.invalid/refund""#,
        ),
    ] {
        let out = compile(
            &CompileRequest::create(SUPPORT)
                .answer("model", r#""mock/echo""#)
                .answer("const.customer_directory", r#""customers.json""#)
                .answer(
                    "const.refund_endpoint",
                    r#""https://refund.example.invalid/refund""#,
                )
                .answer("const.refund_policy", r#""Current approval required""#)
                .answer(key, value),
        )
        .unwrap();
        assert_ne!(out.status, CompileStatus::Ready);
        assert!(out.candidate.is_none());
    }
}

#[test]
fn partial_support_intent_names_the_unmatched_clause() {
    let out = compile(&CompileRequest::create(
        "Look up the customer and draft a reply and send the reply",
    ))
    .unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("send the reply"))
    );
}
