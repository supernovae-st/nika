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
use common::{Judged, Rotating, keys};

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
        "Fais-moi un rapport hebdomadaire des trucs importants",
    ] {
        let out = with_model(intent);
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(out.requested_trigger.is_none(), "{intent}: {out:#?}");
    }
    // The quoted text is written as it is (G2): no model, and no trigger either.
    let intent = "Écris \"réunion chaque lundi\" dans ./out/note.txt";
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    assert!(out.requested_trigger.is_none(), "{intent}: {out:#?}");
    assert!(out.questions.is_empty(), "{intent}: {out:#?}");
}

fn sketch_policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(1)
}

/// The copy as the sketch door's graph: one read, one write of that read (no required hole).
fn copy_graph() -> Vec<String> {
    let graph = json!({"name": "copie-notes", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "reads": ["./notes.md"], "purpose": "the notes"},
        {"id": "write_copy", "verb": "invoke", "tool": "nika:write", "writes": ["./out/copie.md"],
         "with": [{"name": "content", "from": "read_source"}], "purpose": "the copy"}
    ], "questions": [], "gaps": [], "notes": "copy"});
    vec![
        graph.to_string(),
        json!({"fills": [], "notes": "no hole"}).to_string(),
    ]
}

#[tokio::test]
async fn the_native_door_records_the_tail_and_names_two_triggers() {
    // Fresh CREATE is semantic: the sketch door realizes the copy, the tail stays beside it.
    let provider = Rotating::new(copy_graph());
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(
        &CompileRequest::create("Copie ./notes.md dans ./out/copie.md tous les matins")
            .with_authoring_policy(sketch_policy()),
        &Judged::approving(&provider),
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let trigger = out.requested_trigger.as_ref().expect("the tail, recorded");
    assert_eq!(trigger.cadence.as_deref(), Some("daily"));
    let source = out.candidate.as_deref().unwrap();
    assert!(
        !source.contains("matins") && source.contains("./out/copie.md"),
        "{source}"
    );
    let provider = Rotating::new(copy_graph());
    let out = compile_with_provider(
        &CompileRequest::create(
            "Quand un ticket arrive, copie ./notes.md dans ./out/copie.md chaque lundi",
        )
        .with_authoring_policy(sketch_policy()),
        &provider,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(two_triggers(&out), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
}

/// The sketch record's answer round, keyless: the raw request's own compile, no seat.
async fn replayed(request: &CompileRequest) -> nika_compile::CompileOutcome {
    nika_compile_cognition::compile_with_cognition(
        request,
        nika_compile_cognition::Cognition::<nika_compile_cognition::NoProvider>::default(),
    )
    .await
    .unwrap()
}

/// The copy authored through the sketch door under `intent`, judged: its outcome.
async fn authored(intent: &str) -> nika_compile::CompileOutcome {
    let provider = Rotating::new(copy_graph());
    let request = CompileRequest::create(intent).with_authoring_policy(sketch_policy());
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// A stated trigger rides the semantic record in the request's own words (a sentence-initial
/// capital, a lowercase opening, a final cadence) and its replay keeps the requested trigger beside the same candidate, with no call.
#[tokio::test]
async fn a_semantic_record_keeps_the_trigger_in_the_requests_own_words() {
    for (intent, words, cadence) in [
        (
            "Chaque lundi matin, copie ./notes.md dans ./out/copie.md",
            "Chaque lundi matin",
            "weekly",
        ),
        (
            "chaque lundi matin, copie ./notes.md dans ./out/copie.md",
            "chaque lundi matin",
            "weekly",
        ),
        (
            "Copie ./notes.md dans ./out/copie.md tous les matins",
            "tous les matins",
            "daily",
        ),
    ] {
        let out = authored(intent).await;
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        let record = out.provenance.plan.clone().unwrap();
        assert_eq!(
            record["settlement"]["trigger"], words,
            "{intent}: {record:#}"
        );
        assert!(intent.contains(words));
        let source = out.candidate.clone().unwrap();
        assert!(
            !source.contains("lundi") && !source.contains("matins"),
            "{source}"
        );
        let replay = CompileRequest::create(intent)
            .with_authoring_policy(sketch_policy())
            .with_plan(record);
        let again = replayed(&replay).await;
        let trigger = again
            .requested_trigger
            .as_ref()
            .expect("the trigger survives the replay");
        assert_eq!(trigger.cadence.as_deref(), Some(cadence), "{intent}");
        assert_eq!(
            again.candidate.as_deref(),
            Some(source.as_str()),
            "{intent}: {again:#?}"
        );
        assert!(again.provenance.authoring.is_none(), "{intent}: zero calls");
    }
}

/// A record whose trigger is not the request's own words, is empty or of another type, or is
/// null while the request states a cadence, refuses its replay by a static reason: no source is
/// used, no model asked, and the record's words are never repeated.
#[tokio::test]
async fn a_record_trigger_that_is_not_the_requests_own_words_refuses_its_replay() {
    let intent = "Chaque lundi matin, copie ./notes.md dans ./out/copie.md";
    let record = authored(intent).await.provenance.plan.unwrap();
    for (case, trigger, reason) in [
        (
            "foreign",
            json!("zz-trigger-sentinel"),
            "its trigger is not the request's own words",
        ),
        (
            "empty",
            json!(""),
            "its trigger is not the request's own words",
        ),
        (
            "mistyped",
            json!(7),
            "it is not a closed record of this format",
        ),
        (
            "null over a stated cadence",
            json!(null),
            "its trigger omits the cadence the request states",
        ),
    ] {
        let mut forged = record.clone();
        forged["settlement"]["trigger"] = trigger;
        let replay = CompileRequest::create(intent)
            .with_authoring_policy(sketch_policy())
            .with_plan(forged);
        let out = replayed(&replay).await;
        assert!(out.candidate.is_none(), "{case}: {out:#?}");
        let said = format!("{:?} {:?}", out.diagnostics, out.questions);
        assert!(said.contains(reason), "{case}: {said}");
        assert!(!said.contains("zz-trigger-sentinel"), "{case}: {said}");
        assert!(out.provenance.authoring.is_none(), "{case}: zero calls");
    }
}
