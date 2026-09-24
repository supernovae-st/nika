// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A source the request names is opened or the candidate is not READY. The morning audit of
//! 2026-09-22 (both lanes) found « Chaque lundi matin, envoie-moi un récapitulatif des
//! tickets ouverts de ./tickets.json » READY through the deterministic door as one send that
//! posted `{action, target, facts}` and never read the file. The same wording under
//! gpt-5-mini, in six languages, ended in a catch-all clarification because the seat listed
//! the destination and the hour as unknowns — values the compiler already asks or binds.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const MONDAY: &str =
    "Chaque lundi matin, envoie-moi un récapitulatif des tickets ouverts de ./tickets.json";

fn message_mentions(out: &nika_compile::CompileOutcome, needle: &str) -> bool {
    out.diagnostics.iter().any(|d| d.message.contains(needle))
}

#[test]
fn the_deterministic_door_does_not_post_a_recap_of_a_file_it_never_reads() {
    let req = CompileRequest::create(MONDAY).answer(
        "const.send_endpoint",
        r#""https://hooks.example.invalid/recap""#,
    );
    let out = compile(&req).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        message_mentions(
            &out,
            "`./tickets.json` is named by the request and nothing opens it"
        ),
        "{out:#?}"
    );
}

/// The seat's proposal that opens the file: a read, a draft of the recap, the send, and the
/// two unknowns gpt-5-mini lists for this wording.
fn monday_proposal(with_read: bool) -> Value {
    let mut steps = vec![
        json!({"op":"draft","detail":"un récapitulatif des tickets ouverts","evidence":"un récapitulatif des tickets ouverts"}),
    ];
    if with_read {
        steps.insert(
            0,
            json!({"op":"read","detail":"./tickets.json","evidence":"./tickets.json"}),
        );
    }
    json!({"steps": steps,
        "effects":[{"verb":"send","target":"envoie-moi un récapitulatif","policy":"automatic","evidence":"envoie-moi un récapitulatif des tickets ouverts de ./tickets.json"}],
        "obligations":[],"constraints":[],
        "unknowns":["Destinataire et canal d'envoi pour « envoie-moi » (adresse e-mail, webhook…)","Heure exacte du lundi matin et fuseau horaire"],
        "regions":[{"text":"Chaque lundi matin,","role":"context"},
                   {"text":"envoie-moi un récapitulatif des tickets ouverts de ./tickets.json","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_destination_and_an_hour_the_compiler_binds_do_not_end_in_a_clarification() {
    let provider = Provider::new(monday_proposal(true));
    let req = CompileRequest::create(MONDAY).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(keys(&out).contains(&"const.send_endpoint"), "{out:#?}");
    // The merge's applied folds stay in the sample record; the plan carries no unknown.
    let unknowns = out.provenance.plan.as_ref().unwrap()["unknowns"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(unknowns.is_empty(), "{unknowns:?}");
    let trigger = out.requested_trigger.as_ref().expect("a schedule");
    assert_eq!(trigger.cadence.as_deref(), Some("weekly"), "{trigger:?}");
    // Answered, the candidate reads the file before it drafts and posts.
    let provider = Provider::new(monday_proposal(true));
    let req = CompileRequest::create(MONDAY)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#)
        .answer(
            "const.send_endpoint",
            r#""https://hooks.example.invalid/recap""#,
        );
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.as_deref().expect("a candidate");
    assert!(
        candidate.contains("source_path: ./tickets.json"),
        "{candidate}"
    );
    assert!(candidate.contains("nika:read"), "{candidate}");
    assert!(candidate.contains("infer:"), "{candidate}");
    assert!(candidate.contains("hooks.example.invalid"), "{candidate}");
}

#[tokio::test]
async fn a_proposal_that_never_opens_the_named_file_is_unresolved_work() {
    let provider = Provider::new(monday_proposal(false));
    let req = CompileRequest::create(MONDAY).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        message_mentions(
            &out,
            "`./tickets.json` is named by the request and nothing opens it"
        ),
        "{out:#?}"
    );
}
