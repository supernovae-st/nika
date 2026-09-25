// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A retrieval proposed over an outbound effect's clause is that effect (sealed sv3-23 on the
//! lane of 2026-09-22, gpt-5-mini: « envíalas con un POST a http://…/reposicion » was listed
//! as a `fetch` beside the human-first `send` over the same words, and the candidate
//! performed a GET on the endpoint before the gate). One clause is one element.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const BICIS: &str = "lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes), escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas) y envíalas con un POST a http://127.0.0.1:50067/reposicion, pero pídeme confirmación antes de enviar";

fn bicis_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)","evidence":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)"},
        {"op":"compute","detail":"filtrar las estaciones con bicis_disponibles igual a 0 (mantener mismas columnas)","evidence":"bicis_disponibles igual a 0"},
        {"op":"fetch","detail":"POST http://127.0.0.1:50067/reposicion con el cuerpo formado por ./out/vacias.csv","evidence":"envíalas con un POST a http://127.0.0.1:50067/reposicion"}],
        "effects":[{"verb":"write","target":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)","policy":"automatic","evidence":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)"},
                   {"verb":"send","target":"envíalas con un POST a http://127.0.0.1:50067/reposicion","policy":"human_first","evidence":"envíalas con un POST a http://127.0.0.1:50067/reposicion, pero pídeme confirmación antes de enviar"}],
        "obligations":[],"constraints":["mismas columnas"],"unknowns":[],
        "regions":[{"text":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes),","role":"operation"},
                   {"text":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)","role":"effect"},
                   {"text":"y envíalas con un POST a http://127.0.0.1:50067/reposicion,","role":"effect"},
                   {"text":"pero pídeme confirmación antes de enviar","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_fetch_over_the_send_clause_is_the_send() {
    let provider = Provider::new(bicis_proposal());
    let req = CompileRequest::create(BICIS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(out.provenance.plan.is_some(), "a plan is merged: {out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert!(
        !ops.contains(&"fetch"),
        "no GET on the endpoint: {ops:?}\n{out:#?}"
    );
    let effects: Vec<&str> = plan["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["verb"].as_str().unwrap())
        .collect();
    assert_eq!(effects, ["write", "send"], "{out:#?}");
    if let Some(source) = out.candidate.as_deref() {
        assert!(!source.contains("nika:fetch"), "{source}");
    }
}
