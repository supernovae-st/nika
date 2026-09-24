// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A cadence stated at the end of a request (« … chaque lundi », « … tous les matins », « …
//! every Monday ») is the request's schedule: before this law the reader read only heads, the
//! words stayed in the draft's detail, and the one-shot candidate went READY with no trigger
//! despite the requested recurrence. Now the tail is read as a head is and becomes the
//! `requested_trigger`, the candidate's bytes stay those of the request without it, and the
//! existing binding questions and answers apply. A request whose head is another trigger (an
//! event, a different cadence) is two triggers, visibly unresolved on every door; a grouping,
//! a quotation, a negation or an adjective stays what it was.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
    Strategy, TriggerKind, compile,
};
use nika_compile_cognition::compile_with_provider;
use serde_json::json;
use std::time::Duration;

mod common;
use common::{Rotating, keys};

const MODEL: &str = r#""mock/echo""#;

fn with_model(intent: &str) -> CompileOutcome {
    compile(&CompileRequest::create(intent).answer("model", MODEL)).unwrap()
}

fn two_triggers(out: &CompileOutcome) -> bool {
    out.diagnostics.iter().any(|d| {
        d.kind == DiagnosticKind::Unknown
            && d.message.starts_with("the request states two triggers")
    })
}

#[test]
fn a_sentence_final_cadence_is_the_requested_trigger_never_a_silent_one_shot() {
    for (intent, plain, cadence, at) in [
        (
            "Fais-moi un rapport des trucs importants chaque lundi",
            "Fais-moi un rapport des trucs importants",
            "weekly",
            None,
        ),
        (
            "Résume ./notes.md dans ./out/resume.md tous les matins",
            "Résume ./notes.md dans ./out/resume.md",
            "daily",
            None,
        ),
        (
            "Summarize ./notes.md into ./out/summary.md every monday",
            "Summarize ./notes.md into ./out/summary.md",
            "weekly",
            None,
        ),
        (
            "Résume ./notes.md dans ./out/resume.md chaque lundi à 9h",
            "Résume ./notes.md dans ./out/resume.md",
            "weekly",
            Some("09:00"),
        ),
        (
            "Summarize ./notes.md into ./out/summary.md every day at 18:00",
            "Summarize ./notes.md into ./out/summary.md",
            "daily",
            Some("18:00"),
        ),
        (
            "Résume ./notes.md dans ./out/resume.md tous les lundis",
            "Résume ./notes.md dans ./out/resume.md",
            "weekly",
            None,
        ),
        (
            "Summarize ./notes.md into ./out/summary.md each morning",
            "Summarize ./notes.md into ./out/summary.md",
            "daily",
            None,
        ),
        (
            "Résume ./notes.md dans ./out/resume.md toutes les deux heures",
            "Résume ./notes.md dans ./out/resume.md",
            "hourly",
            None,
        ),
    ] {
        let out = with_model(intent);
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{intent}");
        let trigger = out.requested_trigger.as_ref().expect("the stated cadence");
        assert_eq!(trigger.kind, TriggerKind::Schedule, "{intent}");
        assert_eq!(trigger.cadence.as_deref(), Some(cadence), "{intent}");
        assert_eq!(trigger.at.as_deref(), at, "{intent}");
        // The binding values ride beside it, optional, as for a head.
        assert!(
            keys(&out).contains(&"trigger.timezone") && out.questions.iter().all(|q| !q.mandatory),
            "{intent}: {out:#?}"
        );
        // The bytes are the request's without its cadence: trigger-agnostic.
        assert_eq!(out.candidate, with_model(plain).candidate, "{intent}");
    }
}

#[test]
fn the_binding_answers_and_the_record_carry_the_tail_as_they_carry_a_head() {
    let intent = "Résume ./notes.md dans ./out/resume.md chaque lundi à 9h";
    let answered = compile(
        &CompileRequest::create(intent)
            .answer("model", MODEL)
            .answer("trigger.timezone", r#""Europe/Paris""#)
            .answer("trigger.missed", r#""sauter""#),
    )
    .unwrap();
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
    let trigger = answered.requested_trigger.as_ref().unwrap();
    assert_eq!(trigger.timezone.as_deref(), Some("Europe/Paris"));
    assert_eq!(trigger.missed.as_deref(), Some("sauter"));
    // The first round's record keeps the tail; its answer round replays the same requirement.
    let first = compile(&CompileRequest::create(intent)).unwrap();
    let plan = first.provenance.plan.clone().unwrap();
    assert_eq!(plan["trigger"], "chaque lundi à 9h", "{plan:#}");
    let replayed = compile(
        &CompileRequest::create(intent)
            .with_plan(plan)
            .answer("model", MODEL),
    )
    .unwrap();
    assert_eq!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert_eq!(
        replayed.requested_trigger.as_ref().unwrap().at.as_deref(),
        Some("09:00")
    );
    assert_eq!(replayed.candidate, with_model(intent).candidate);
}

#[test]
fn a_head_beside_a_different_cadence_is_two_triggers_visibly_unresolved() {
    for intent in [
        "Quand un ticket arrive, rédige un accusé de réception dans ./out/accuse.md chaque lundi",
        "Chaque lundi, résume ./notes.md dans ./out/resume.md tous les matins",
    ] {
        let out = with_model(intent);
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(two_triggers(&out), "{intent}: {out:#?}");
        assert!(
            out.questions
                .iter()
                .any(|q| q.key == "intent.clarification" && q.mandatory),
            "{intent}: {out:#?}"
        );
    }
    // A head that already says the tail keeps it, READY.
    let out = with_model("Chaque lundi, résume ./notes.md dans ./out/resume.md chaque lundi");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.requested_trigger.as_ref().unwrap().cadence.as_deref(),
        Some("weekly")
    );
}

#[test]
fn a_grouping_a_quotation_or_an_adjective_is_unchanged() {
    // READY with no trigger before this law, and still (the e6bc576b witness rows G01, G03,
    // G04, Q01, A01).
    for intent in [
        "Résume les ventes de chaque mois de ./ventes.csv dans ./out/resume.md",
        "Résume ./notes.md dans ./out/resume.md pour chaque mois",
        "Résume les tickets de ./tickets.json par jour dans ./out/resume.md",
        "Écris \"réunion chaque lundi\" dans ./out/note.txt",
        "Fais-moi un rapport hebdomadaire des trucs importants",
    ] {
        let out = with_model(intent);
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(out.requested_trigger.is_none(), "{intent}: {out:#?}");
    }
}

fn native() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Only)
        .with_repairs(1)
}

const COPY: &str = r#"nika: copie-notes
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./notes.md"]
    write: ["./out/copie.md"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args:
        path: "./notes.md"
  write_copy:
    with:
      content: "${{ tasks.read_source.output }}"
    invoke:
      tool: "nika:write"
      args:
        path: "./out/copie.md"
        content: "${{ with.content }}"
        overwrite: true
        create_dirs: true
"#;

#[tokio::test]
async fn the_native_door_records_the_tail_and_names_two_triggers() {
    let answer =
        json!({"candidate": COPY, "questions": [], "gaps": [], "notes": "copy"}).to_string();
    let provider = Rotating::new(vec![answer.clone()]);
    let out = compile_with_provider(
        &CompileRequest::create("Copie ./notes.md dans ./out/copie.md tous les matins")
            .with_authoring_policy(native()),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let trigger = out.requested_trigger.as_ref().expect("the tail, recorded");
    assert_eq!(trigger.cadence.as_deref(), Some("daily"));
    let provider = Rotating::new(vec![answer]);
    let out = compile_with_provider(
        &CompileRequest::create(
            "Quand un ticket arrive, copie ./notes.md dans ./out/copie.md chaque lundi",
        )
        .with_authoring_policy(native()),
        &provider,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(two_triggers(&out), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
}
