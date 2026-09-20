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
fn partial_support_intent_falls_through_to_the_general_reader() {
    // The exact grammar does not own "send the reply"; the general reader keeps the
    // effect as a bound question instead of stopping at the grammar's fragment.
    let out = compile(&CompileRequest::create(
        "Look up the customer and draft a reply and send the reply",
    ))
    .unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.questions.iter().any(|q| q.key == "const.send_endpoint"),
        "{out:#?}"
    );
    let plan = out.provenance.plan.expect("general plan");
    assert_eq!(plan["effects"][0]["verb"], "send");
    assert_eq!(plan["effects"][0]["evidence"], "send the reply");
}

#[test]
fn bare_model_id_is_not_an_explicit_provider_binding() {
    let out = compile(
        &CompileRequest::create(SUPPORT)
            .answer("model", r#""gpt-4o""#)
            .answer("const.customer_directory", r#""customers.json""#)
            .answer(
                "const.refund_endpoint",
                r#""https://refund.example.invalid/refunds""#,
            )
            .answer("const.refund_policy", r#""cap 100 EUR""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(out.questions.iter().any(|q| q.key == "model"));
    assert!(out.diagnostics.iter().any(|d| d.target == "model"));
}

#[test]
fn rejected_binding_answers_keep_their_stable_question() {
    let base = || {
        CompileRequest::create(SUPPORT)
            .answer("model", r#""mock/echo""#)
            .answer("const.refund_policy", r#""cap 100 EUR""#)
    };
    let glob = compile(
        &base()
            .answer("const.customer_directory", r#""customers-*.json""#)
            .answer(
                "const.refund_endpoint",
                r#""https://refund.example.invalid/refund""#,
            ),
    )
    .unwrap();
    assert_eq!(glob.status, CompileStatus::Incomplete);
    assert!(glob.candidate.is_none());
    assert!(
        glob.questions
            .iter()
            .any(|q| q.key == "const.customer_directory"),
        "{glob:#?}"
    );
    for endpoint in [
        r#""https://user:secret@refund.example.invalid/refund""#,
        r#""https://refund.example.invalid/refund#approved""#,
        r#""https://refund.example.invalid/refund?api_key=SECRET""#,
        r#""https://refund.example.invalid/refund?v=2&access_token=abc""#,
        r#""http://refund.example.invalid/refund""#,
        r#""ftp://refund.example.invalid/refund""#,
        r#""not a url""#,
    ] {
        let out = compile(
            &base()
                .answer("const.customer_directory", r#""customers.json""#)
                .answer("const.refund_endpoint", endpoint),
        )
        .unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{endpoint}");
        assert!(out.candidate.is_none(), "{endpoint}");
        assert!(
            out.questions
                .iter()
                .any(|q| q.key == "const.refund_endpoint"),
            "{endpoint}: {out:#?}"
        );
    }
}

#[test]
fn versioned_query_and_loopback_development_endpoints_stay_explicit_bindings() {
    for (endpoint, host) in [
        (
            r#""https://refund.example.invalid/refund?api-version=2024-01""#,
            "refund.example.invalid",
        ),
        (r#""http://127.0.0.1:8080/refund""#, "127.0.0.1"),
        (r#""http://localhost:8080/refund""#, "localhost"),
    ] {
        let out = compile(
            &CompileRequest::create(SUPPORT)
                .answer("model", r#""mock/echo""#)
                .answer("const.customer_directory", r#""customers.json""#)
                .answer("const.refund_policy", r#""cap 100 EUR""#)
                .answer("const.refund_endpoint", endpoint),
        )
        .unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{endpoint}: {out:#?}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        assert_eq!(doc["permits"]["net"]["http"], json!([host]), "{endpoint}");
    }
}
