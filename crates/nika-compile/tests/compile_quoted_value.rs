// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Three small laws measured on the sealed lane6 (nika-6b394e81, gpt-5-mini): the quotes a
//! request wears around a value are not the value (« no » compared as the six characters kept
//! no row — a READY that wrote an empty file), a POST body of computed rows is data a draft
//! only serializes, and a draft over the very write clause with no language word beside the
//! computed rows is that serialization too.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const CLINICA: &str = "Hola, ¿podrías por favor leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada) y preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas? Luego armoniza el tono de los nombres en ./out/nombres.md. ¡Muchas gracias!";

fn clinica_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./clinica/citas.csv (columnas cita, paciente, hora, confirmada)","evidence":"leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada)"},
        {"op":"compute","detail":"las citas cuya columna confirmada sea « no »","evidence":"las citas cuya columna confirmada sea « no »",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"confirmada","op":"==","value":"« no »","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","order":"","columns":["cita","paciente","hora","confirmada"],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":"el tono de los nombres","evidence":"armoniza el tono de los nombres"}],
        "effects":[{"verb":"write","target":"./out/sin-confirmar.csv","policy":"automatic","evidence":"preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas"},
                   {"verb":"write","target":"./out/nombres.md","policy":"automatic","evidence":"armoniza el tono de los nombres en ./out/nombres.md"}],
        "obligations":[],"constraints":["mismas columnas"],"unknowns":[],
        "regions":[{"text":"Hola, ¿podrías por favor leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada)","role":"operation"},
                   {"text":"y preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas?","role":"effect"},
                   {"text":"Luego armoniza el tono de los nombres en ./out/nombres.md.","role":"operation"},
                   {"text":"¡Muchas gracias!","role":"context"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn the_quotes_around_a_value_are_not_the_value() {
    let provider = Provider::new(clinica_proposal());
    let req = CompileRequest::create(CLINICA).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let rules = plan["rules"].as_array().unwrap();
    let jq = rules[0]["jq"].as_str().unwrap_or_default();
    assert!(
        jq.contains(".confirmada == \"no\""),
        "the value is « no » without its guillemets: {jq}\n{out:#?}"
    );
}

const BICIS: &str = "lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes), escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas) y envíalas con un POST a http://127.0.0.1:50067/reposicion, pero pídeme confirmación antes de enviar";

fn bicis_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)","evidence":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)"},
        {"op":"compute","detail":"filtrar filas con bicis_disponibles igual a 0 y conservar las mismas columnas","evidence":"bicis_disponibles igual a 0"},
        {"op":"draft","detail":"preparar cuerpo POST con las estaciones filtradas hacia http://127.0.0.1:50067/reposicion","evidence":"envíalas con un POST a http://127.0.0.1:50067/reposicion"}],
        "effects":[{"verb":"write","target":"./out/vacias.csv","policy":"automatic","evidence":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)"},
                   {"verb":"send","target":"envíalas con un POST a http://127.0.0.1:50067/reposicion","policy":"human_first","evidence":"envíalas con un POST a http://127.0.0.1:50067/reposicion, pero pídeme confirmación antes de enviar"}],
        "obligations":[],"constraints":["mismas columnas"],"unknowns":[],
        "regions":[{"text":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes),","role":"operation"},
                   {"text":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)","role":"effect"},
                   {"text":"y envíalas con un POST a http://127.0.0.1:50067/reposicion,","role":"effect"},
                   {"text":"pero pídeme confirmación antes de enviar","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_draft_of_the_post_body_of_computed_rows_is_a_serialization() {
    let provider = Provider::new(bicis_proposal());
    let req = CompileRequest::create(BICIS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert!(
        !ops.contains(&"draft"),
        "the body is the rows: {ops:?}\n{out:#?}"
    );
    assert!(!keys(&out).contains(&"model"), "{out:#?}");
}

const CALCIO: &str = "leggi ./calcio/partite.csv (colonne partita, esito), conta quante partite hanno ciascun valore di esito e scrivi ./out/esiti.csv con le colonne esito,numero (una riga per valore). Poi armonizza il tono dei nomi in ./out/nomi.md.";

fn calcio_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./calcio/partite.csv (colonne partita, esito)","evidence":"leggi ./calcio/partite.csv (colonne partita, esito)"},
        {"op":"compute","detail":"conta quante partite hanno ciascun valore di esito","evidence":"conta quante partite hanno ciascun valore di esito",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[],
                        "aggregations":[{"as":"numero","op":"count","field":""}],"group_by":"esito","sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":"./out/esiti.csv con le colonne esito,numero (una riga per valore)","evidence":"scrivi ./out/esiti.csv con le colonne esito,numero (una riga per valore)"},
        {"op":"draft","detail":"il tono dei nomi","evidence":"armonizza il tono dei nomi"}],
        "effects":[{"verb":"write","target":"./out/esiti.csv","policy":"automatic","evidence":"scrivi ./out/esiti.csv con le colonne esito,numero (una riga per valore)"},
                   {"verb":"write","target":"./out/nomi.md","policy":"automatic","evidence":"armonizza il tono dei nomi in ./out/nomi.md"}],
        "obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"leggi ./calcio/partite.csv (colonne partita, esito),","role":"operation"},
                   {"text":"conta quante partite hanno ciascun valore di esito","role":"operation"},
                   {"text":"e scrivi ./out/esiti.csv con le colonne esito,numero (una riga per valore).","role":"effect"},
                   {"text":"Poi armonizza il tono dei nomi in ./out/nomi.md.","role":"operation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_draft_over_the_write_clause_with_no_language_word_is_a_serialization() {
    let provider = Provider::new(calcio_proposal());
    let req = CompileRequest::create(CALCIO).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let drafts: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["op"] == "draft")
        .map(|o| o["evidence"].as_str().unwrap())
        .collect();
    assert_eq!(
        drafts,
        ["armonizza il tono dei nomi"],
        "the rows are written as they are, the tone is language work: {out:#?}"
    );
}
