// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A lexical reading never imprisons a seat's understanding, and never grants an effect:
//! - a negation whose scope is another predicate (« don't forget to email », « do not hesitate
//!   to email », « never fail to post », « n'hésite pas à envoyer ») bans nothing: the reader
//!   leaves the clause to cognition, and a seat realizes it without a new approval gate;
//! - a ban of another object beside a request of the same verb is targeted, never a
//!   verb-merged contradiction (« post the digest …; never post the raw CSV »);
//! - a contradiction between the request's own words for one effect stays the human's (R4 S0,
//!   superseding the earlier « reaches the seat » reading): refused, no seat call, no candidate.
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
use common::{Judged, Rotating};

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

/// The note sent as the sketch door's graph and its fills: read, then post to the stated hook,
/// behind a review when `gated`.
fn send_note(gated: bool) -> Vec<String> {
    let note = json!([{"name": "note", "from": "read_note"}]);
    let mut tasks = vec![
        json!({"id": "read_note", "verb": "invoke", "tool": "nika:read",
        "reads": ["./note.txt"], "purpose": "the note"}),
    ];
    let mut send = json!({"id": "send", "verb": "invoke", "tool": "nika:notify",
        "hosts": ["hooks.example.test"], "with": note, "purpose": "post the note"});
    let mut fills = vec![
        json!({"task": "send", "field": "args.target", "value": "https://hooks.example.test/in"}),
        json!({"task": "send", "field": "args.message", "value": "${{ with.note }}"}),
    ];
    if gated {
        tasks.push(
            json!({"id": "review", "verb": "invoke", "tool": "nika:prompt", "with": note,
            "purpose": "ask before sending"}),
        );
        send["gated_by"] = json!("review");
        fills.push(json!({"task": "review", "field": "args.message",
            "value": "Envoyer cette note ? ${{ with.note }}"}));
    }
    tasks.push(send);
    let graph = json!({"name": "send-note", "tasks": tasks, "questions": [], "gaps": [],
        "notes": "read → send"});
    vec![
        graph.to_string(),
        json!({"fills": fills, "notes": "fills"}).to_string(),
    ]
}

#[tokio::test]
async fn the_seat_realizes_what_the_reader_left_to_cognition_without_a_new_gate() {
    let intent = "Read ./note.txt and don't forget to post it to https://hooks.example.test/in.";
    let provider = Rotating::new(send_note(false));
    let req = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Sketch, 1));
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&req, &Judged::approving(&provider))
        .await
        .unwrap();
    let rounds = rounds(&out);
    assert_eq!(
        rounds,
        vec![Vec::<String>::new(), Vec::<String>::new()],
        "accepted as written, no gate demanded"
    );
    assert_eq!(accepted(&out), true, "{out:#?}");
    // The intended automatic effect: the send, ungated; no review was invented.
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    assert_eq!(
        doc["tasks"]["send"]["invoke"]["args"]["target"],
        "https://hooks.example.test/in"
    );
    assert!(doc["tasks"]["send"].get("when").is_none(), "{doc:#}");
    assert!(doc["tasks"].get("review").is_none(), "{doc:#}");
}

#[tokio::test]
async fn a_banned_destination_is_refused_even_behind_a_gate() {
    let intent = "Lis ./note.txt et ne l'envoie jamais à https://hooks.example.test/in.";
    let provider = Rotating::new(send_note(true));
    let req = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Sketch, 1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    // A nonempty, targeted prohibition: never a vacuous `all` over no round.
    assert!(!rounds.is_empty(), "{out:#?}");
    assert!(
        rounds
            .iter()
            .flatten()
            .any(|m| m.starts_with("PROHIBITED EFFECT")),
        "{rounds:?}"
    );
    assert_ne!(accepted(&out), true, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
}

#[tokio::test]
async fn a_contradiction_of_the_words_stays_the_humans_and_never_reaches_a_seat() {
    let intent = "Lis ./note.txt et envoie-la à https://hooks.example.test/in; ne l'envoie jamais à https://hooks.example.test/in.";
    // A seat would realize the send if it were asked (the earlier contract let it choose).
    let provider = Rotating::new(send_note(false));
    let req = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Escalate, 1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(
        provider.calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "no paid call reads the contradiction"
    );
    assert_eq!(out.status, CompileStatus::Refused, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let stated = out
        .diagnostics
        .iter()
        .find(|d| {
            d.message
                .starts_with("Contradictory instructions for `send`")
        })
        .expect("the contradiction is stated");
    assert_eq!(stated.kind, nika_compile::DiagnosticKind::RequiresHuman);
}
