// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Two composer and merge laws measured on the sealed-v3 treatment lane (2026-09-22): a
//! `draft` the reader guessed over a write clause is the write of a computed value, and a
//! `revision_check` proposed over a gate phrase is the gate the effect's policy carries.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider, outcome_document};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy, route};

/// sv3-11: the reader reads « write just the number » as a draft; the seat computes the
/// count and writes it, with the write effect anchored on the path alone.
const KINGFISHER: &str = "hey can u count how many kingfisher sightings there r in ./birds/sightings.csv (species column, its written 'Kingfisher') and write just the number, nothing else, to ./out/kingfisher.txt thx";

fn kingfisher_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"read ./birds/sightings.csv","evidence":"./birds/sightings.csv"},
        {"op":"compute","detail":"count how many rows have species = 'Kingfisher' in ./birds/sightings.csv","evidence":"species column, its written 'Kingfisher'",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[{"field":"species","op":"eq","value":"Kingfisher","value_field":""}],"group_by":"",
                        "aggregations":[{"field":"","op":"count","as":"number","round":""}],"sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[]}}],
        "effects":[{"verb":"write","target":"write ./out/kingfisher.txt","policy":"automatic","evidence":"./out/kingfisher.txt"}],
        "obligations":[],"constraints":["nothing else"],"unknowns":[],
        "regions":[{"text":"hey can u count how many kingfisher sightings there r in ./birds/sightings.csv (species column, its written 'Kingfisher')","role":"operation"},
                   {"text":"and write just the number, nothing else, to ./out/kingfisher.txt thx","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_guessed_draft_over_a_write_clause_is_the_write_of_a_computed_value() {
    let provider = Provider::new(kingfisher_proposal());
    let req = CompileRequest::create(KINGFISHER).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics.iter().any(|d| d
            .message
            .contains("dropped the recognized operation `draft`")),
        "{out:#?}"
    );
    let doc = outcome_document(&out);
    assert!(route(&doc).contains("compose: single"), "{}", route(&doc));
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
}

/// sv3-23: the seat proposed a `revision_check` over « pídeme confirmación antes de
/// enviar », which is the gate the send's `human_first` policy already carries.
const BICIS: &str = "lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes), escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas) y envíalas con un POST a http://127.0.0.1:64910/reposicion, pero pídeme confirmación antes de enviar.";

fn bicis_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"lee ./bicis/estaciones.csv","evidence":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes)"},
        {"op":"compute","detail":"filtra filas con bicis_disponibles igual a 0; conserva columnas estacion, bicis_disponibles, anclajes","evidence":"bicis_disponibles igual a 0",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[{"field":"bicis_disponibles","op":"eq","value":"0","value_field":""}],"group_by":"",
                        "aggregations":[],"sort_by":"","order":"","columns":["estacion","bicis_disponibles","anclajes"],"derived":[],"limit":"","renames":[]}}],
        "effects":[{"verb":"write","target":"escribe en ./out/vacias.csv","policy":"automatic","evidence":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)"},
                   {"verb":"send","target":"envíalas con un POST a http://127.0.0.1:64910/reposicion","policy":"human_first","evidence":"envíalas con un POST a http://127.0.0.1:64910/reposicion"}],
        "obligations":[{"kind":"revision_check","value":null,"evidence":"pídeme confirmación antes de enviar"}],
        "constraints":["mismas columnas"],"unknowns":[],
        "regions":[{"text":"lee ./bicis/estaciones.csv (estacion, bicis_disponibles, anclajes),","role":"operation"},
                   {"text":"escribe en ./out/vacias.csv las estaciones con bicis_disponibles igual a 0 (mismas columnas)","role":"effect"},
                   {"text":"y envíalas con un POST a http://127.0.0.1:64910/reposicion,","role":"effect"},
                   {"text":"pero pídeme confirmación antes de enviar.","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_revision_check_over_a_gate_phrase_is_the_gate() {
    let provider = Provider::new(bicis_proposal());
    let req = CompileRequest::create(BICIS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("revision_check")
                && d.message.contains("no retrievable source")),
        "{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let doc = outcome_document(&out);
    assert!(route(&doc).contains("compose: single"), "{}", route(&doc));
    assert_eq!(
        doc["provenance"]["plan"]["obligations"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "{doc}"
    );
}
