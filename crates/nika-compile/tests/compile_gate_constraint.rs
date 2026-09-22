// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A proposed constraint that is the gate phrase itself (sealed lane9 on nika-5d7ed9dc,
//! gpt-5-mini: sv3-23 « pero pídeme confirmación antes de enviar », sv3-13 « Please ask me
//! for confirmation before writing the file, I want to approve it first. », sv3-03
//! « Demandez-moi confirmation avant tout envoi ; rien ne doit partir sans mon accord. »)
//! beside the effect the reading already gated is that gate: the ledger filed it as a format
//! duty nobody carried and refused READY for a silent obligation. Folded.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

fn constraints(out: &nika_compile::CompileOutcome) -> Vec<String> {
    out.provenance.plan.as_ref().unwrap()["constraints"]
        .as_array()
        .map(|a| a.iter().map(|c| c.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn silent_obligation(out: &nika_compile::CompileOutcome) -> bool {
    out.diagnostics.iter().any(|d| {
        d.message
            .contains("no element of the compiled workflow carries it")
    })
}

const BICIS: &str = "lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes), escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas) y envíalas con un POST a http://127.0.0.1:53604/reposicion, pero pídeme confirmación antes de enviar";

fn bicis_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)","evidence":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)"},
        {"op":"compute","detail":"las estaciones con bicis_disponibles igual a 0 (mismas columnas)","evidence":"las estaciones con bicis_disponibles igual a 0 (mismas columnas)",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"bicis_disponibles","op":"==","value":"0","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","order":"","columns":["estacion","bicis_disponibles","anclajes"],"derived":[],"limit":"","renames":[]}}],
        "effects":[
        {"verb":"write","target":"./out/vacias.csv","policy":"automatic","evidence":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)"},
        {"verb":"send","target":"http://127.0.0.1:53604/reposicion","policy":"human_first","evidence":"envíalas con un POST a http://127.0.0.1:53604/reposicion"}],
        "obligations":[],
        "constraints":["pero pídeme confirmación antes de enviar","mismas columnas"],"unknowns":[],
        "regions":[{"text":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes),","role":"operation"},
                   {"text":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)","role":"effect"},
                   {"text":"y envíalas con un POST a http://127.0.0.1:53604/reposicion,","role":"effect"},
                   {"text":"pero pídeme confirmación antes de enviar","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_proposed_constraint_that_is_the_gate_phrase_is_the_gate() {
    let provider = Provider::new(bicis_proposal());
    let req = CompileRequest::create(BICIS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(!silent_obligation(&out), "{out:#?}");
    assert!(
        !constraints(&out)
            .iter()
            .any(|c| c.contains("pídeme confirmación")),
        "{out:#?}"
    );
}

const ROASTERY: &str = "Hello! Would you mind reading ./roastery/batches.csv (columns batch, bean, weight_kg, roast_level) and computing the total weight_kg for each roast_level? I'd like the result written to ./out/by-roast.csv with exactly two columns, roast_level and total_kg, one row per roast level. Please ask me for confirmation before writing the file, I want to approve it first. Thank you so much!";

fn roastery_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./roastery/batches.csv (columns batch, bean, weight_kg, roast_level)","evidence":"reading ./roastery/batches.csv (columns batch, bean, weight_kg, roast_level)"},
        {"op":"compute","detail":"the total weight_kg for each roast_level","evidence":"computing the total weight_kg for each roast_level",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[],
                        "aggregations":[{"as":"total_kg","op":"sum","field":"weight_kg"}],"group_by":"roast_level","sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[]}}],
        "effects":[
        {"verb":"write","target":"./out/by-roast.csv","policy":"human_first","evidence":"I'd like the result written to ./out/by-roast.csv with exactly two columns, roast_level and total_kg, one row per roast level"}],
        "obligations":[],
        "constraints":["exactly two columns, roast_level and total_kg, one row per roast level","Please ask me for confirmation before writing the file, I want to approve it first."],"unknowns":[],
        "regions":[{"text":"Hello!","role":"context"},
                   {"text":"Would you mind reading ./roastery/batches.csv (columns batch, bean, weight_kg, roast_level)","role":"operation"},
                   {"text":"and computing the total weight_kg for each roast_level?","role":"operation"},
                   {"text":"I'd like the result written to ./out/by-roast.csv with exactly two columns, roast_level and total_kg, one row per roast level.","role":"effect"},
                   {"text":"Please ask me for confirmation before writing the file, I want to approve it first.","role":"policy"},
                   {"text":"Thank you so much!","role":"context"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn an_english_gate_listed_as_a_constraint_is_carried_by_the_gated_write() {
    let provider = Provider::new(roastery_proposal());
    let req = CompileRequest::create(ROASTERY).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!silent_obligation(&out), "{out:#?}");
    assert!(
        !constraints(&out)
            .iter()
            .any(|c| c.contains("ask me for confirmation")),
        "{out:#?}"
    );
}
