// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One clause is one step: a seat that lists the same clause under seven operations states
//! one thing seven times (measured on the sealed-v3 lane of 2026-09-22, gpt-5-mini over
//! sv3-01), and a retrieval or a validation over a write clause is the write itself.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const PRETS: &str = "Veuillez lire le fichier ./bibliotheque/prets.csv (colonnes pret_id, lecteur, titre, jours_retard), conserver uniquement les prêts dont le retard dépasse strictement 14 jours, et écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv.";

/// The live proposal, shortened: the write clause listed under every operation the schema
/// offers, with one detail.
fn seven_times() -> Value {
    let clause =
        "écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv";
    let detail = "Écrire les lignes filtrées, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv";
    let mut steps = vec![
        json!({"op":"read","detail":"Lire le fichier local ./bibliotheque/prets.csv.","evidence":"lire le fichier ./bibliotheque/prets.csv"}),
        json!({"op":"compute","detail":"Filtrer les lignes pour ne conserver que celles où jours_retard > 14","evidence":"conserver uniquement les prêts dont le retard dépasse strictement 14 jours",
               "computation":{"present":true,"polarity":"keep","join":"and","clauses":[{"field":"jours_retard","op":"gt","value":"14","value_field":""}],"group_by":"",
                              "aggregations":[],"sort_by":"","order":"","columns":["pret_id","lecteur","titre","jours_retard"],"derived":[],"limit":"","renames":[]}}),
    ];
    for op in [
        "validate", "explore", "fetch", "lookup", "classify", "extract", "search",
    ] {
        steps.push(json!({"op":op,"detail":detail,"evidence":clause}));
    }
    json!({"steps":steps,
           "effects":[{"verb":"write","target":"./out/retards.csv","policy":"automatic","evidence":clause}],
           "obligations":[],"constraints":["avec les mêmes colonnes et dans le même ordre"],"unknowns":[],
           "regions":[{"text":"Veuillez lire le fichier ./bibliotheque/prets.csv (colonnes pret_id, lecteur, titre, jours_retard),","role":"operation"},
                      {"text":"conserver uniquement les prêts dont le retard dépasse strictement 14 jours,","role":"operation"},
                      {"text":"et écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv.","role":"effect"}],
           "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_clause_listed_under_seven_operations_is_one_write() {
    let provider = Provider::new(seven_times());
    let req = CompileRequest::create(PRETS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(!keys(&out).contains(&"model"), "{out:#?}");
    let ops: Vec<&str> = out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["read", "compute"], "{out:#?}");
}

/// sv3-26: the seat proposed an `explore` over « Antes de escribir el archivo, pregúntame y
/// espera mi aprobación » beside the `human_first` write that gate already dominates.
const CITAS: &str = "Hola, ¿podrías por favor leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada) y preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas? Antes de escribir el archivo, pregúntame y espera mi aprobación.";

fn citas_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./clinica/citas.csv","evidence":"leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada)"},
        {"op":"compute","detail":"Filtrar filas cuya columna confirmada sea « no », mismas columnas","evidence":"con las citas cuya columna confirmada sea « no »",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[{"field":"confirmada","op":"eq","value":"no","value_field":""}],"group_by":"",
                        "aggregations":[],"sort_by":"","order":"","columns":["cita","paciente","hora","confirmada"],"derived":[],"limit":"","renames":[]}},
        {"op":"explore","detail":"Preguntar al solicitante y esperar su aprobación antes de escribir el archivo","evidence":"Antes de escribir el archivo, pregúntame y espera mi aprobación."}],
        "effects":[{"verb":"write","target":"./out/sin-confirmar.csv","policy":"human_first","evidence":"preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas"}],
        "obligations":[],"constraints":["mismas columnas"],"unknowns":[],
        "regions":[{"text":"Hola, ¿podrías por favor","role":"context"},
                   {"text":"leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada)","role":"operation"},
                   {"text":"y preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas?","role":"effect"},
                   {"text":"Antes de escribir el archivo, pregúntame y espera mi aprobación.","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_step_over_a_gate_phrase_is_the_gate_the_effect_carries() {
    let provider = Provider::new(citas_proposal());
    let req = CompileRequest::create(CITAS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"model"), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let source = out.candidate.as_deref().unwrap();
    assert!(source.contains("nika:prompt"), "{source}");
    assert!(!source.contains("agent:"), "{source}");
}
