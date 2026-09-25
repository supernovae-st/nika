// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A Portuguese schedule (sealed lane9 on nika-5d7ed9dc, gpt-5-mini, sv3-56): « todas as
//! manhãs às 7h, lê ./coworking/reservas.csv … » never became the trigger — the reader's
//! prefixes, the daily words and the time introducer knew no Portuguese — so the seat filed
//! the schedule as a constraint, the ledger made it a format duty nobody carried, and READY
//! was refused. The leading clause is the trigger, daily at 07:00, and the proposed
//! constraint over the same words is folded.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, outcome_document};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const RESERVAS: &str = "todas as manhãs às 7h, lê ./coworking/reservas.csv (reserva,mesa,pessoa,data) e escreve em ./out/hoje.csv as linhas com data igual a 2026-09-22, mesmas colunas";

fn reservas_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./coworking/reservas.csv (reserva,mesa,pessoa,data)","evidence":"lê ./coworking/reservas.csv (reserva,mesa,pessoa,data)"},
        {"op":"compute","detail":"as linhas com data igual a 2026-09-22, mesmas colunas","evidence":"as linhas com data igual a 2026-09-22, mesmas colunas",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"data","op":"==","value":"2026-09-22","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","order":"","columns":["reserva","mesa","pessoa","data"],"derived":[],"limit":"","renames":[]}}],
        "effects":[{"verb":"write","target":"./out/hoje.csv","policy":"automatic","evidence":"escreve em ./out/hoje.csv as linhas com data igual a 2026-09-22, mesmas colunas"}],
        "obligations":[],
        "constraints":["todas as manhãs às 7h","mesmas colunas"],"unknowns":[],
        "regions":[{"text":"todas as manhãs às 7h,","role":"context"},
                   {"text":"lê ./coworking/reservas.csv (reserva,mesa,pessoa,data)","role":"operation"},
                   {"text":"e escreve em ./out/hoje.csv as linhas com data igual a 2026-09-22, mesmas colunas","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_portuguese_morning_schedule_is_the_trigger_daily_at_seven() {
    let provider = Provider::new(reservas_proposal());
    let req = CompileRequest::create(RESERVAS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let doc = outcome_document(&out);
    let trigger = &doc["requested_trigger"];
    assert_eq!(trigger["kind"], "schedule", "{doc:#}");
    assert_eq!(trigger["cadence"], "daily", "{doc:#}");
    assert_eq!(trigger["at"], "07:00", "{doc:#}");
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(
        plan["trigger"].as_str(),
        Some("todas as manhãs às 7h"),
        "{plan:#?}"
    );
    let filed: Vec<&str> = plan["constraints"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        !filed.iter().any(|c| c.contains("manhãs")),
        "the schedule is the trigger, not a format duty: {plan:#?}"
    );
    assert!(
        !out.diagnostics.iter().any(|d| d
            .message
            .contains("no element of the compiled workflow carries it")),
        "{out:#?}"
    );
}
