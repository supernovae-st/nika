// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A lexical reading never imprisons a seat's understanding, and never grants an effect:
//! - a negation whose scope is another predicate (« don't forget to email », « do not hesitate
//!   to email », « never fail to post », « n'hésite pas à envoyer ») bans nothing: the reader
//!   leaves the clause to cognition, and a seat realizes it without a new approval gate;
//! - a ban of another object beside a request of the same verb is targeted, never a
//!   verb-merged contradiction (« post the digest …; never post the raw CSV »);
//! - a contradiction between the request's own words for one effect reaches the seat instead of
//!   dead-ending, and what the seat realizes of it is stated to the review.
//!
//! Negative controls: a negation that reaches its effect stays a ban (« never email », « ne pas
//! envoyer », « do not ever send », « ne l'envoie jamais »), a ban of the same object or
//! destination stays a contradiction, and a banned destination is refused even behind a gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode, compile,
};
use nika_compile_cognition::compile_with_provider;
use nika_compile_reader::lexicon;
use nika_compile_reader::plan::EffectPolicy;
use serde_json::{Value, json};
use std::time::Duration;

mod common;
use common::Rotating;

/// The policies of every effect the reader states for a request.
fn policies(intent: &str) -> Vec<EffectPolicy> {
    lexicon::read(intent)
        .plan
        .effects
        .iter()
        .map(|e| e.policy)
        .collect()
}

#[test]
fn a_negation_whose_scope_is_another_predicate_bans_nothing() {
    for intent in [
        "Don't forget to email the report to ops@example.test.",
        "Read ./note.txt and don't forget to post it to https://hooks.example.test/in.",
        "Do not hesitate to email the report to ops@example.test.",
        "Never fail to post the digest to https://hooks.example.test/x.",
        "N'hésite pas à envoyer le rapport à ops@example.test.",
    ] {
        let reading = lexicon::read(intent);
        let found: Vec<EffectPolicy> = reading.plan.effects.iter().map(|e| e.policy).collect();
        assert!(
            !found.contains(&EffectPolicy::Forbidden),
            "{intent}: {found:?}"
        );
        assert!(
            !reading.unresolved.is_empty(),
            "the clause is cognition's to read: {intent}"
        );
    }
    // An elided infinitive (« d'envoyer ») is no effect word the reader places: nothing turns
    // the clause into a ban either.
    let french = policies("N'oublie pas d'envoyer le rapport à ops@example.test.");
    assert!(!french.contains(&EffectPolicy::Forbidden), "{french:?}");
    // A negation that reaches its effect (at most one particle between) stays a ban.
    for intent in [
        "Never email the report to ops@example.test.",
        "Ne pas envoyer le rapport à ops@example.test.",
        "Do not ever send the report to ops@example.test.",
        "Lis ./note.txt et ne l'envoie jamais à https://hooks.example.test/in.",
    ] {
        assert_eq!(policies(intent), vec![EffectPolicy::Forbidden], "{intent}");
    }
}

#[test]
fn a_ban_of_another_object_is_targeted_and_the_same_object_still_contradicts() {
    let found =
        policies("Post the digest to https://hooks.example.test/x; never post the raw CSV.");
    assert!(
        found.contains(&EffectPolicy::Automatic) && found.contains(&EffectPolicy::Forbidden),
        "{found:?}"
    );
    assert!(!found.contains(&EffectPolicy::Conflict), "{found:?}");
    for intent in [
        "Post the digest to https://hooks.example.test/x; never post it.",
        "Post the digest to https://hooks.example.test/x; never post the digest.",
        "Post the digest to https://hooks.example.test/x; never post anything to https://hooks.example.test/x.",
    ] {
        assert!(
            policies(intent).contains(&EffectPolicy::Conflict),
            "{intent}: {:?}",
            policies(intent)
        );
    }
}

#[test]
fn the_deterministic_door_no_longer_refuses_or_bans_the_two_readings() {
    let asked = compile(&CompileRequest::create(
        "Read ./report.md and don't forget to email it to ops@example.test.",
    ))
    .unwrap();
    assert_ne!(asked.status, CompileStatus::Refused, "{asked:#?}");
    assert!(
        !asked
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Prohibited effect omitted")),
        "{asked:#?}"
    );
    assert!(
        asked.candidate.is_some() || !asked.questions.is_empty(),
        "a proposal or a question, never a silent ban: {asked:#?}"
    );
    let targeted = compile(&CompileRequest::create(
        "Read ./digest.md and post it to https://hooks.example.test/x; never post the raw CSV.",
    ))
    .unwrap();
    assert_ne!(targeted.status, CompileStatus::Refused, "{targeted:#?}");
    assert!(
        !targeted
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Contradictory instructions")),
        "{targeted:#?}"
    );
    // The same destination both asked and banned stays a visible contradiction there.
    let contradiction = compile(&CompileRequest::create(
        "Lis ./note.txt et envoie-la à https://hooks.example.test/in; ne l'envoie jamais à https://hooks.example.test/in.",
    ))
    .unwrap();
    assert_eq!(
        contradiction.status,
        CompileStatus::Refused,
        "{contradiction:#?}"
    );
}

fn policy(native: NativeMode, repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(repairs)
}

fn answer(candidate: &str) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "read → send"}).to_string()
}

fn rounds(out: &CompileOutcome) -> Vec<Vec<String>> {
    out.provenance.decision.as_ref().unwrap()["native"]["rounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|round| {
            round["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|d| d["message"].as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

fn accepted(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["native"]["accepted"].clone()
}

fn send_note(send: &str) -> String {
    format!(
        r#"nika: send-note
permits:
  tools: ["nika:read", "nika:prompt", "nika:fetch"]
  fs:
    read: ["./note.txt"]
  net:
    http: ["hooks.example.test"]
tasks:
  read_note:
    invoke:
      tool: "nika:read"
      args: {{ path: "./note.txt" }}
{send}"#
    )
}

const UNGATED: &str = r#"  send:
    with: { note: "${{ tasks.read_note.output }}" }
    invoke:
      tool: "nika:fetch"
      args: { url: "https://hooks.example.test/in", method: POST, body: "${{ with.note }}" }
"#;

const GATED: &str = r#"  review:
    with: { note: "${{ tasks.read_note.output }}" }
    invoke:
      tool: "nika:prompt"
      args: { message: "Envoyer cette note ? ${{ with.note }}" }
  send:
    with: { approved: "${{ tasks.review.output }}", note: "${{ tasks.read_note.output }}" }
    when: "${{ with.approved == true }}"
    invoke:
      tool: "nika:fetch"
      args: { url: "https://hooks.example.test/in", method: POST, body: "${{ with.note }}" }
"#;

#[tokio::test]
async fn the_seat_realizes_what_the_reader_left_to_cognition_without_a_new_gate() {
    let intent = "Read ./note.txt and don't forget to post it to https://hooks.example.test/in.";
    let provider = Rotating::new(vec![answer(&send_note(UNGATED))]);
    let req = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Only, 1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert_eq!(
        rounds,
        vec![Vec::<String>::new()],
        "accepted as written, no gate demanded"
    );
    assert_eq!(accepted(&out), true, "{out:#?}");
}

#[tokio::test]
async fn a_banned_destination_is_refused_even_behind_a_gate() {
    let intent = "Lis ./note.txt et ne l'envoie jamais à https://hooks.example.test/in.";
    let provider = Rotating::new(vec![answer(&send_note(GATED))]);
    let req = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Only, 1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert!(
        rounds
            .iter()
            .all(|r| r.iter().any(|m| m.starts_with("PROHIBITED EFFECT"))),
        "{rounds:?}"
    );
    assert_ne!(accepted(&out), true, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}

#[tokio::test]
async fn a_contradiction_of_the_words_reaches_the_seat_and_is_stated_to_the_review() {
    let intent = "Lis ./note.txt et envoie-la à https://hooks.example.test/in; ne l'envoie jamais à https://hooks.example.test/in.";
    let provider = Rotating::new(vec![answer(&send_note(UNGATED))]);
    let req = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Escalate, 1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let route = out.provenance.decision.as_ref().unwrap()["route"].to_string();
    assert!(route.contains("native: escalated"), "{route}");
    assert_eq!(accepted(&out), true, "{out:#?}");
    let stated = out
        .diagnostics
        .iter()
        .find(|d| d.target == "reading")
        .expect("the seat's reading of the contradiction is stated");
    assert_eq!(
        stated.kind,
        nika_compile::DiagnosticKind::Applied,
        "never a refusal"
    );
    assert!(stated.message.contains("`send`"), "{}", stated.message);
}
