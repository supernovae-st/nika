// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A request that wants its work repeated without saying when (« Fais-moi un rapport des
//! trucs importants régulièrement », the independent audit's witness D) never becomes READY on
//! a model answer alone: the recurrence is the plan's trigger, a schedule whose cadence is a
//! mandatory question. The answer is read as a stated cadence is read and recorded on
//! `requested_trigger`, never in the candidate's bytes; `manual` starts each run by hand. A
//! stated cadence, and a request with no recurrence word, stay exactly as they were.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, Strategy, TriggerKind,
    TriggerStatus, compile,
};

const WITNESS: &str = "Fais-moi un rapport des trucs importants régulièrement";
const MODEL: &str = r#""mock/echo""#;

fn cadence_asked(out: &CompileOutcome) -> bool {
    out.questions
        .iter()
        .any(|q| q.key == "trigger.cadence" && q.mandatory)
}

fn answered(intent: &str, cadence: &str) -> CompileOutcome {
    compile(
        &CompileRequest::create(intent)
            .answer("model", MODEL)
            .answer("trigger.cadence", cadence),
    )
    .unwrap()
}

#[test]
fn a_recurrence_without_its_cadence_asks_it_and_a_model_answer_alone_is_never_ready() {
    let first = compile(&CompileRequest::create(WITNESS)).unwrap();
    assert_eq!(first.provenance.strategy, Some(Strategy::Hot), "{first:#?}");
    assert!(cadence_asked(&first), "{first:#?}");
    let plan = first.provenance.plan.clone().unwrap();
    assert_eq!(plan["trigger"], "régulièrement", "{plan:#}");
    // The audit's second round, fresh and replayed from the first round's record: only the
    // model is answered, and the recurrence is still asked.
    for request in [
        CompileRequest::create(WITNESS).answer("model", MODEL),
        CompileRequest::create(WITNESS)
            .with_plan(plan)
            .answer("model", MODEL),
    ] {
        let out = compile(&request).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(cadence_asked(&out), "{out:#?}");
        let trigger = out.requested_trigger.as_ref().expect("the recurrence");
        assert_eq!(trigger.kind, TriggerKind::Schedule);
        assert_eq!(trigger.status, TriggerStatus::RequiresBinding);
        assert_eq!(trigger.source_hint.as_deref(), Some("régulièrement"));
        assert_eq!(trigger.cadence, None);
        assert_eq!(trigger.at, None);
    }
}

#[test]
fn the_cadence_answer_is_read_as_a_stated_cadence_and_never_enters_the_bytes() {
    let weekly = answered(WITNESS, r#""chaque lundi à 9h""#);
    assert_eq!(weekly.status, CompileStatus::Ready, "{weekly:#?}");
    let trigger = weekly.requested_trigger.as_ref().unwrap();
    assert_eq!(trigger.kind, TriggerKind::Schedule);
    assert_eq!(trigger.cadence.as_deref(), Some("weekly"));
    assert_eq!(trigger.at.as_deref(), Some("09:00"));
    assert!(weekly.questions.iter().all(|q| !q.mandatory), "{weekly:#?}");
    assert!(
        weekly
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Applied && d.target == "trigger.cadence"),
        "{weekly:#?}"
    );
    let candidate = weekly.candidate.as_deref().unwrap();
    assert!(
        !candidate.contains("lundi")
            && !candidate.contains("09:00")
            && !candidate.contains("weekly"),
        "{candidate}"
    );
    let daily = answered(WITNESS, r#""every day at 18:00""#);
    assert_eq!(daily.status, CompileStatus::Ready, "{daily:#?}");
    let trigger = daily.requested_trigger.as_ref().unwrap();
    assert_eq!(trigger.cadence.as_deref(), Some("daily"));
    assert_eq!(trigger.at.as_deref(), Some("18:00"));
    // The bytes are trigger-agnostic: the same candidate whatever the cadence.
    assert_eq!(daily.candidate, weekly.candidate);
}

#[test]
fn manual_starts_each_run_by_hand_and_an_answer_without_a_cadence_keeps_the_question() {
    let manual = answered(WITNESS, r#""manuel""#);
    assert_eq!(manual.status, CompileStatus::Ready, "{manual:#?}");
    let trigger = manual.requested_trigger.as_ref().unwrap();
    assert_eq!(trigger.kind, TriggerKind::Manual);
    assert_eq!(trigger.status, TriggerStatus::Satisfied);
    assert!(
        manual.questions.is_empty(),
        "a run started by hand binds nothing: {manual:#?}"
    );
    assert_eq!(
        manual.candidate,
        answered(WITNESS, r#""chaque lundi à 9h""#).candidate
    );
    for vague in [r#""bientôt""#, r#""souvent""#, "3"] {
        let out = answered(WITNESS, vague);
        assert_ne!(out.status, CompileStatus::Ready, "{vague}: {out:#?}");
        assert!(cadence_asked(&out), "{vague}: {out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::Missed && d.target == "trigger.cadence"),
            "{vague}: {out:#?}"
        );
        let trigger = out.requested_trigger.as_ref().unwrap();
        assert_eq!(trigger.kind, TriggerKind::Schedule, "{vague}");
        assert_eq!(trigger.cadence, None, "{vague}");
    }
}

#[test]
fn the_other_forms_of_the_witness_ask_too() {
    for intent in [
        "Régulièrement, fais-moi un rapport des trucs importants",
        "Fais-moi régulièrement un rapport des trucs importants",
        "Résume ./notes.md de temps en temps dans ./out/resume.md",
        "Summarize ./notes.md into ./out/summary.md on a regular basis",
    ] {
        let out = compile(&CompileRequest::create(intent).answer("model", MODEL)).unwrap();
        assert_eq!(
            out.provenance.strategy,
            Some(Strategy::Hot),
            "{intent}: {out:#?}"
        );
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(cadence_asked(&out), "{intent}: {out:#?}");
    }
}

#[test]
fn a_head_that_took_the_trigger_leaves_the_recurrence_unbound_and_the_request_is_never_hot() {
    // Ready before this law, the event recorded and « régulièrement » gone without a word.
    let intent =
        "Quand un ticket arrive, rédige régulièrement un accusé de réception dans ./out/accuse.md";
    let out = compile(&CompileRequest::create(intent).answer("model", MODEL)).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification" && q.mandatory),
        "{out:#?}"
    );
    let route = out.provenance.decision.as_ref().unwrap()["route"].to_string();
    assert!(
        route.contains("hot rejected") && route.contains("régulièrement"),
        "{route}"
    );
    // Without the recurrence the same event request is READY as it was.
    let plain = "Quand un ticket arrive, rédige un accusé de réception dans ./out/accuse.md";
    let out = compile(&CompileRequest::create(plain).answer("model", MODEL)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.requested_trigger.as_ref().unwrap().kind,
        TriggerKind::Event
    );
}

#[test]
fn a_stated_cadence_and_a_request_without_recurrence_are_unchanged() {
    let stated = "Chaque lundi à 9h, résume ./notes.md dans ./out/resume.md";
    let out = compile(&CompileRequest::create(stated).answer("model", MODEL)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.questions
            .iter()
            .all(|q| !q.mandatory && q.key != "trigger.cadence"),
        "{out:#?}"
    );
    let trigger = out.requested_trigger.as_ref().unwrap();
    assert_eq!(trigger.cadence.as_deref(), Some("weekly"));
    assert_eq!(trigger.at.as_deref(), Some("09:00"));
    // No question owns a cadence answer there: it is not applied, and says so.
    let unasked = answered(stated, r#""every day""#);
    assert_ne!(unasked.status, CompileStatus::Ready, "{unasked:#?}");
    assert_eq!(
        unasked
            .requested_trigger
            .as_ref()
            .unwrap()
            .cadence
            .as_deref(),
        Some("weekly")
    );
    for plain in [
        "Résume ./notes.md dans ./out/resume.md",
        "Résume ./notes.md dans ./out/resume.md en vérifiant la régularité des dépenses",
    ] {
        let out = compile(&CompileRequest::create(plain).answer("model", MODEL)).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{plain}: {out:#?}");
        assert!(out.requested_trigger.is_none(), "{plain}");
        assert!(out.questions.is_empty(), "{plain}: {out:#?}");
    }
}
