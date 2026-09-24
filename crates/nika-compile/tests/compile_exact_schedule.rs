// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile, outcome_document};

#[test]
fn exact_schedule_is_additive_and_an_answer_survives_native_plan_replay() {
    let intent = "Régulièrement, lis ./notes/brief.md et écris-le dans ./out/copy.md";
    let initial = compile(&CompileRequest::create(intent)).unwrap();
    let plan = initial.provenance.plan.clone().unwrap();
    for (answer, cron) in [
        ("chaque mardi à 9h", "0 9 * * 2"),
        ("every Friday at 10", "0 10 * * 5"),
        ("every 2 hours", "0 */2 * * *"),
    ] {
        let request = CompileRequest::create(intent)
            .with_plan(plan.clone())
            .answer("trigger.cadence", serde_json::to_string(answer).unwrap());
        let out = compile(&request).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{out:?}");
        let trigger = out.requested_trigger.as_ref().unwrap();
        assert_eq!(trigger.cron.as_deref(), Some(cron));
        assert_eq!(trigger.source_hint.as_deref(), Some("régulièrement"));
        assert!(!out.candidate.as_deref().unwrap().contains(cron));
        let wire = outcome_document(&out);
        assert_eq!(wire["requested_trigger"]["cron"], cron);
        assert_eq!(wire["requested_trigger"]["status"], "requires_binding");
        assert!(wire["requested_trigger"]["timezone"].is_null());
    }
}

#[test]
fn the_coarse_label_is_not_an_exact_schedule_or_a_binding() {
    for (phrase, coarse, exact) in [
        ("Every 2 hours", "hourly", Some("0 */2 * * *")),
        ("Every hour", "hourly", Some("0 * * * *")),
        ("Every Tuesday at 9", "weekly", Some("0 9 * * 2")),
        ("Every Friday at 9", "weekly", Some("0 9 * * 5")),
        ("Every day", "daily", None),
        ("Every 2 days at 9", "daily", None),
    ] {
        let intent = format!("{phrase}, read ./a.md and write it to ./b.md");
        let out = compile(&CompileRequest::create(intent)).unwrap();
        let trigger = out.requested_trigger.as_ref().unwrap();
        assert_eq!(trigger.cadence.as_deref(), Some(coarse));
        assert_eq!(trigger.cron.as_deref(), exact);
        assert!(trigger.timezone.is_none());
    }
}
