// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A period five cron fields cannot hold (R4 A5 · C1): « every other monday at 09:00 » was
//! READY with a `weekly` requested trigger, a schedule the request never stated. It is now a
//! schedule whose cadence is asked before READY: the label stays empty, no cron is proposed,
//! the time of day is kept, and the human answers a cadence a schedule binds or « manual ».
//! Another unbindable period is refused as an answer; the weekly, weekday and clock-interval
//! schedules a binding holds stay READY exactly as before.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, TriggerKind, TriggerStatus,
    compile,
};

mod common;

const TAIL: &str =
    "read ./tickets.json, keep only the rows whose status is open and write them to ./open.json";

fn request(trigger: &str) -> CompileRequest {
    let tickets: &[&str] = &["id", "status"];
    CompileRequest::create(format!("{trigger}, {TAIL}"))
        .with_knowledge(common::observed(&[("./tickets.json", tickets)]))
}

fn cadence_question(out: &CompileOutcome) -> Option<&nika_compile::CompileQuestion> {
    out.questions.iter().find(|q| q.key == "trigger.cadence")
}

#[test]
fn every_other_monday_asks_its_cadence_and_is_never_recorded_weekly() {
    for (phrase, read_as_trigger) in [
        ("Every other Monday at 09:00", true),
        ("Every 2 weeks at 9", true),
        ("Every second Friday at 10", true),
        ("Every other week", true),
        ("Biweekly on Monday at 8", false),
        ("Un lundi sur deux à 9h", false),
        ("Tous les quinze jours à 8h", false),
        ("Cada dos semanas a las 9", false),
        ("Jede zweite Woche um 9 Uhr", false),
    ] {
        let out = compile(&request(phrase)).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{phrase}: {out:#?}");
        let Some(trigger) = out.requested_trigger.as_ref() else {
            // A clause the reader does not take as a trigger stays unresolved: never READY.
            assert!(!read_as_trigger, "{phrase}: {out:#?}");
            continue;
        };
        assert!(cadence_question(&out).is_some(), "{phrase}: {out:#?}");
        let question = cadence_question(&out).unwrap();
        assert!(question.mandatory, "{phrase}");
        assert!(
            question.label.contains("cannot bind"),
            "{phrase}: {}",
            question.label
        );
        assert_eq!(trigger.kind, TriggerKind::Schedule, "{phrase}");
        assert_eq!(trigger.cadence, None, "{phrase}: never a coarse label");
        assert_eq!(trigger.cron, None, "{phrase}: never a narrowed cron");
    }
    let out = compile(&request("Every other Monday at 09:00")).unwrap();
    let trigger = out.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(
        trigger.at.as_deref(),
        Some("09:00"),
        "the time of day is kept"
    );
    assert_eq!(
        trigger.source_hint.as_deref(),
        Some("every other monday at 09:00")
    );
}

#[test]
fn the_answered_cadence_binds_and_another_unbindable_period_is_asked_again() {
    let weekly = compile(
        &request("Every other Monday at 09:00")
            .answer("trigger.cadence", r#""every Monday at 09:00""#),
    )
    .unwrap();
    assert_eq!(weekly.status, CompileStatus::Ready, "{weekly:#?}");
    let trigger = weekly.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.cadence.as_deref(), Some("weekly"));
    assert_eq!(trigger.cron.as_deref(), Some("0 9 * * 1"));
    assert_eq!(trigger.at.as_deref(), Some("09:00"));
    assert!(cadence_question(&weekly).is_none(), "{weekly:#?}");

    let manual =
        compile(&request("Every other Monday at 09:00").answer("trigger.cadence", r#""manual""#))
            .unwrap();
    assert_eq!(manual.status, CompileStatus::Ready, "{manual:#?}");
    let trigger = manual.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.kind, TriggerKind::Manual);
    assert_eq!(trigger.status, TriggerStatus::Satisfied);

    let again = compile(
        &request("Every other Monday at 09:00")
            .answer("trigger.cadence", r#""every other Monday at 9""#),
    )
    .unwrap();
    assert_ne!(again.status, CompileStatus::Ready, "{again:#?}");
    assert!(cadence_question(&again).is_some(), "{again:#?}");
    assert!(
        again
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Missed
                && d.target == "trigger.cadence"
                && d.message.contains("cannot bind as said")
                && d.message.contains("start date")),
        "{again:#?}"
    );
    let trigger = again.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.cadence, None);
}

#[test]
fn a_schedule_the_binding_holds_stays_ready_with_its_exact_fields() {
    for (phrase, cadence, cron) in [
        ("Every Monday at 09:00", Some("weekly"), Some("0 9 * * 1")),
        ("Chaque lundi à 9h", Some("weekly"), Some("0 9 * * 1")),
        ("Every weekday at 8", Some("weekdays"), Some("0 8 * * 1-5")),
        ("Every 2 hours", Some("hourly"), Some("0 */2 * * *")),
        (
            "Toutes les deux heures",
            Some("hourly"),
            Some("0 */2 * * *"),
        ),
        ("Every day at noon", Some("daily"), Some("0 12 * * *")),
    ] {
        let out = compile(&request(phrase)).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{phrase}: {out:#?}");
        assert!(cadence_question(&out).is_none(), "{phrase}: {out:#?}");
        let trigger = out.requested_trigger.as_ref().expect("a trigger");
        assert_eq!(trigger.cadence.as_deref(), cadence, "{phrase}");
        assert_eq!(trigger.cron.as_deref(), cron, "{phrase}");
    }
}
