// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The money law as a decision: an automatic refund, payment or order is one closed choice
//! (`effect.<verb>.approval`: `human_first` | `forbidden`), never the catch-all clarification, on
//! the deterministic path and under a seat alike; the answer sets the policy at assembly.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, QuestionType, compile, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

/// The reader recognizes every clause; the refund states no approval.
const AUTOMATIC: &str = "Consulte le client, classe le problème, puis harmonise le ton de la réponse. Rembourse ensuite le client.";

fn proposal() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"Consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le client","policy":"automatic","evidence":"Rembourse ensuite le client"}],
           "obligations":[],"constraints":[],"unknowns":[],
           "regions":[{"text":"Consulte le client","role":"operation"},{"text":"classe le problème","role":"operation"},{"text":"harmonise le ton de la réponse","role":"operation"},{"text":"Rembourse ensuite le client.","role":"effect"}],
           "approval_bypass":{"present":false,"evidence":""}})
}

fn answered(request: CompileRequest) -> CompileRequest {
    request
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer("const.refund_policy", r#"{"cap":100,"currency":"EUR"}"#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
}

#[tokio::test]
async fn an_automatic_refund_is_one_closed_choice_never_the_catch_all() {
    let provider = Provider::new(proposal());
    let req = CompileRequest::create(AUTOMATIC).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let question = out
        .questions
        .iter()
        .find(|q| q.key == "effect.refund.approval")
        .expect("the approval choice is asked");
    assert_eq!(question.answer_type, QuestionType::Choice);
    assert!(question.mandatory);
    let offered: Vec<&str> = question.options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(offered, ["human_first", "forbidden"]);
    assert!(question.label.contains("moves money"), "{}", question.label);
    // The other bindings of the same plan are asked in the same round; the refund's own
    // policy and endpoint wait for the decision (moot when it is forbidden).
    assert!(keys(&out).contains(&"const.customer_directory"), "{out:#?}");
    assert!(!keys(&out).contains(&"const.refund_policy"), "{out:#?}");
    // The recorded plan keeps the policy as it was read: the decision is not the record.
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["effects"][0]["policy"],
        "automatic"
    );
}

#[tokio::test]
async fn human_first_answer_gates_the_refund_and_forbidden_omits_it() {
    let provider = Provider::new(proposal());
    let gated = compile_with_provider(
        &answered(CompileRequest::create(AUTOMATIC).with_authoring_policy(policy()))
            .answer("effect.refund.approval", r#""human_first""#),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(gated.status, CompileStatus::Ready, "{gated:#?}");
    let source = gated.candidate.as_deref().unwrap();
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    assert_eq!(
        doc["tasks"]["refund_review"]["invoke"]["tool"],
        "nika:prompt"
    );
    assert_eq!(
        doc["tasks"]["refund"]["when"],
        "${{ with.approved == true }}"
    );
    assert!(
        gated
            .diagnostics
            .iter()
            .any(|d| d.message.contains("`refund` gated by explicit answer")),
        "{gated:#?}"
    );
    // Forbidden: the refund's own bindings are moot and are not answered.
    let omitted = compile_with_provider(
        &CompileRequest::create(AUTOMATIC)
            .with_authoring_policy(policy())
            .answer("model", r#""mock/echo""#)
            .answer("const.customer_directory", r#""customers.json""#)
            .answer("effect.refund.approval", r#""forbidden""#),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(omitted.status, CompileStatus::Ready, "{omitted:#?}");
    let source = omitted.candidate.as_deref().unwrap();
    assert!(!source.contains("refund.example.invalid"), "{source}");
    assert!(!source.contains("nika:prompt"), "{source}");
    assert!(
        omitted
            .diagnostics
            .iter()
            .any(|d| d.message.contains("`refund` omitted by explicit answer")),
        "{omitted:#?}"
    );
}

#[tokio::test]
async fn a_wrong_approval_answer_is_missed_and_the_choice_stays() {
    let provider = Provider::new(proposal());
    let out = compile_with_provider(
        &answered(CompileRequest::create(AUTOMATIC).with_authoring_policy(policy()))
            .answer("effect.refund.approval", r#""automatic""#),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"effect.refund.approval"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "effect.refund.approval"
                && d.message.contains("human_first · forbidden")),
        "{out:#?}"
    );
}

#[test]
fn the_deterministic_door_asks_the_same_choice_without_a_seat() {
    // No seat is permitted: the reader's own automatic refund is decided the same way.
    let out = compile(&CompileRequest::create(AUTOMATIC)).unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(
        keys(&out).contains(&"effect.refund.approval")
            || keys(&out).contains(&"intent.clarification"),
        "{out:#?}"
    );
    assert!(
        !out.diagnostics.iter().any(|d| d
            .message
            .contains("moves money without a prior human approval")),
        "{out:#?}"
    );
}
