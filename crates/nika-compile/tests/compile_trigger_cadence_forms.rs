// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An interval of weeks is bound once its start date is stated: the words become the
//! cadence grammar's anchored form on `requested_trigger`, and the cadence question is not
//! asked; without the date (or with a date on another weekday) the question stays and names
//! the start date it needs. An answer that states the start date binds the schedule.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, TriggerKind, compile};

mod common;

const REST: &str =
    ", read ./tickets.json, keep only the rows whose status is open and write them to ./open.json";

fn request(trigger: &str) -> CompileRequest {
    let tickets: &[&str] = &["id", "status"];
    CompileRequest::create(format!("{trigger}{REST}"))
        .with_knowledge(common::observed(&[("./tickets.json", tickets)]))
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

#[test]
fn an_interval_of_weeks_with_its_start_date_is_bound_without_a_cadence_question() {
    let out = compile(&request("Every other Monday at 09:00 from 2026-10-05")).unwrap();
    let trigger = out.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.kind, TriggerKind::Schedule);
    assert_eq!(
        trigger.cron.as_deref(),
        Some("every 2 weeks from 2026-10-05 09:00"),
        "{out:#?}"
    );
    assert_eq!(trigger.cadence, None, "a multiple keeps no coarse label");
    assert_eq!(trigger.at.as_deref(), Some("09:00"));
    assert!(!keys(&out).contains(&"trigger.cadence"), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    nika_cadence::registry::Cadence::parse(&format!(
        "TZ=Europe/Paris {}",
        trigger.cron.as_deref().unwrap()
    ))
    .expect("the binding validates it with the cadence grammar");
}

#[test]
fn without_its_start_date_the_cadence_is_asked_and_an_answer_with_the_date_binds_it() {
    let asked = compile(&request("Every other Monday at 09:00")).unwrap();
    let trigger = asked.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.cron, None, "{asked:#?}");
    let question = asked
        .questions
        .iter()
        .find(|q| q.key == "trigger.cadence")
        .expect("the cadence is asked");
    assert!(question.mandatory);
    assert!(
        question.label.contains("start date"),
        "the question names what is missing: {}",
        question.label
    );
    let answered = compile(&request("Every other Monday at 09:00").answer(
        "trigger.cadence",
        "\"every other monday at 09:00 from 2026-10-05\"",
    ))
    .unwrap();
    let trigger = answered.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(
        trigger.cron.as_deref(),
        Some("every 2 weeks from 2026-10-05 09:00"),
        "{answered:#?}"
    );
    assert_eq!(trigger.cadence, None);
    assert!(
        !keys(&answered).contains(&"trigger.cadence"),
        "{answered:#?}"
    );
}

#[test]
fn a_month_end_or_a_start_date_read_at_the_door_is_bound_whole() {
    for (trigger, cron) in [
        ("On the last day of every month at 18:00", "0 18 L * *"),
        ("Every month on the last day at 18:00", "0 18 L * *"),
        (
            "Every 2 weeks on Monday at 9:00, starting 2026-10-05",
            "every 2 weeks from 2026-10-05 09:00",
        ),
        (
            "Every two weeks from 2026-10-05 at 09:00",
            "every 2 weeks from 2026-10-05 09:00",
        ),
    ] {
        let out = compile(&request(trigger)).unwrap();
        let bound = out.requested_trigger.as_ref().expect("a trigger");
        assert_eq!(bound.kind, TriggerKind::Schedule, "{trigger}");
        assert_eq!(bound.cron.as_deref(), Some(cron), "{trigger}: {out:#?}");
        assert!(
            !keys(&out).contains(&"trigger.cadence"),
            "{trigger}: {out:#?}"
        );
        assert_eq!(out.status, CompileStatus::Ready, "{trigger}: {out:#?}");
    }
    // French heads reach the same forms (the clause itself is not judged here).
    for (trigger, cron) in [
        ("Le dernier jour de chaque mois à 18h", "0 18 L * *"),
        (
            "Toutes les deux semaines le lundi à 9h à partir du 2026-10-05",
            "every 2 weeks from 2026-10-05 09:00",
        ),
    ] {
        let tickets: &[&str] = &["id", "status"];
        let request = CompileRequest::create(format!(
            "{trigger}, lis ./tickets.json, garde seulement les lignes dont le statut est open et écris-les dans ./open.json"
        ))
        .with_knowledge(common::observed(&[("./tickets.json", tickets)]));
        let out = compile(&request).unwrap();
        let bound = out.requested_trigger.as_ref().expect("a trigger");
        assert_eq!(bound.cron.as_deref(), Some(cron), "{trigger}: {out:#?}");
        assert!(
            !keys(&out).contains(&"trigger.cadence"),
            "{trigger}: {out:#?}"
        );
    }
}

#[test]
fn a_start_date_on_another_weekday_is_never_bound() {
    // 2026-10-06 is a Tuesday: the words name Monday, so no proposal, the question stays.
    let out = compile(&request("Every other Monday at 09:00 from 2026-10-06")).unwrap();
    let trigger = out.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.cron, None, "{out:#?}");
    assert!(keys(&out).contains(&"trigger.cadence"), "{out:#?}");
}
