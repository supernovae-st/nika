// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A draft of the computed number alone, or one that composes the CSV body of extracted
//! rows, only serializes what a computation produces (sealed lane5 on nika-e02d0aac,
//! gpt-5-mini: « écrivez ce nombre seul dans ./out/fermees.txt » and « Comporre il contenuto
//! CSV con intestazione esatta » were kept as drafts, and the candidates asked a model where
//! the corpus expects zero calls). The number beside prose stays a draft.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const STATION: &str = "Lisez ./station/remontees.csv (colonnes remontee, etat), comptez les remontées dont l'état est « fermée » et écrivez ce nombre seul dans ./out/fermees.txt.";

fn station_proposal(draft_detail: &str) -> Value {
    json!({"steps":[
        {"op":"read","detail":"Lisez ./station/remontees.csv (colonnes remontee, etat)","evidence":"Lisez ./station/remontees.csv (colonnes remontee, etat)"},
        {"op":"compute","detail":"comptez les remontées dont l'état est « fermée »","evidence":"comptez les remontées dont l'état est « fermée »",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"etat","op":"==","value":"fermée","value_field":""}],
                        "aggregations":[{"as":"fermees","op":"count","field":""}],"group_by":"","sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":draft_detail,"evidence":"écrivez ce nombre seul dans ./out/fermees.txt"}],
        "effects":[{"verb":"write","target":"./out/fermees.txt","policy":"automatic","evidence":"écrivez ce nombre seul dans ./out/fermees.txt"}],
        "obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"Lisez ./station/remontees.csv (colonnes remontee, etat),","role":"operation"},
                   {"text":"comptez les remontées dont l'état est « fermée »","role":"operation"},
                   {"text":"et écrivez ce nombre seul dans ./out/fermees.txt.","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_draft_of_the_computed_number_alone_is_folded() {
    let provider = Provider::new(station_proposal(
        "écrivez ce nombre seul dans ./out/fermees.txt.",
    ));
    let req = CompileRequest::create(STATION).with_authoring_policy(policy());
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
        "the number is written as it is: {out:#?}"
    );
    assert!(
        !keys(&out).contains(&"model"),
        "no language step, no model asked: {out:#?}"
    );
}

const STATION_NOTE: &str = "Lisez ./station/remontees.csv (colonnes remontee, etat), comptez les remontées dont l'état est « fermée » et rédigez une courte note avec ce nombre et un commentaire dans ./out/fermees.txt.";

/// The request itself asks for prose (« rédigez une courte note avec ce nombre et un
/// commentaire »): language work, a draft. A request that asks for the number alone folds
/// the draft whatever the seat's detail says (the request's words decide).
#[tokio::test]
async fn a_draft_of_the_number_beside_prose_stays_a_draft() {
    let mut proposal = station_proposal("rédige une courte note avec ce nombre et un commentaire");
    proposal["steps"][2]["evidence"] =
        json!("rédigez une courte note avec ce nombre et un commentaire dans ./out/fermees.txt");
    proposal["effects"][0]["evidence"] =
        json!("rédigez une courte note avec ce nombre et un commentaire dans ./out/fermees.txt");
    proposal["regions"][2]["text"] = json!(
        "et rédigez une courte note avec ce nombre et un commentaire dans ./out/fermees.txt."
    );
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(STATION_NOTE).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert!(ops.contains(&"draft"), "prose is language work: {out:#?}");
}

#[tokio::test]
async fn a_draft_that_composes_the_csv_body_is_folded() {
    let intent = "Leggi ./apicoltura/arnie.json ed estrai per ogni oggetto i campi arnia, peso_kg, regina, poi scrivi ./out/arnie.csv con intestazione esatta arnia,peso_kg,regina.";
    let proposal = json!({"steps":[
        {"op":"read","detail":"./apicoltura/arnie.json","evidence":"Leggi ./apicoltura/arnie.json"},
        {"op":"extract","detail":"per ogni oggetto i campi arnia, peso_kg, regina","evidence":"estrai per ogni oggetto i campi arnia, peso_kg, regina"},
        {"op":"draft","detail":"Comporre il contenuto CSV con intestazione esatta arnia,peso_kg,regina e una riga per ogni oggetto estratto","evidence":"scrivi ./out/arnie.csv con intestazione esatta arnia,peso_kg,regina"}],
        "effects":[{"verb":"write","target":"./out/arnie.csv","policy":"automatic","evidence":"scrivi ./out/arnie.csv con intestazione esatta arnia,peso_kg,regina"}],
        "obligations":[],"constraints":["intestazione esatta arnia,peso_kg,regina"],"unknowns":[],
        "regions":[{"text":"Leggi ./apicoltura/arnie.json","role":"operation"},
                   {"text":"ed estrai per ogni oggetto i campi arnia, peso_kg, regina,","role":"operation"},
                   {"text":"poi scrivi ./out/arnie.csv con intestazione esatta arnia,peso_kg,regina.","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}});
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(intent).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert!(
        !ops.contains(&"draft"),
        "the rows are written as they are: {out:#?}"
    );
}

const KINGFISHER: &str = "hey can u count how many kingfisher sightings there r in ./birds/sightings.csv (species column, its written 'Kingfisher') and write just the number, nothing else, to ./out/kingfisher.txt thx";

/// Sealed lane9 (nika-5d7ed9dc, gpt-5-mini), sv3-11: the seat's draft detail carried no
/// serializing verb (« draft text containing only the numeric count, no other text or
/// labels »), the request's own words did (« write just the number, nothing else »): the
/// number is written as the computation produced it, no model is asked.
#[tokio::test]
async fn the_requests_own_words_fold_a_draft_of_the_number_alone() {
    let proposal = json!({"steps":[
        {"op":"read","detail":"./birds/sightings.csv (species column)","evidence":"count how many kingfisher sightings there r in ./birds/sightings.csv (species column, its written 'Kingfisher')"},
        {"op":"compute","detail":"count of rows whose species is 'Kingfisher'","evidence":"count how many kingfisher sightings there r in ./birds/sightings.csv (species column, its written 'Kingfisher')",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"species","op":"==","value":"Kingfisher","value_field":""}],
                        "aggregations":[{"as":"count","op":"count","field":""}],"group_by":"","sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":"draft text containing only the numeric count, no other text or labels","evidence":"write just the number, nothing else, to ./out/kingfisher.txt"}],
        "effects":[{"verb":"write","target":"./out/kingfisher.txt","policy":"automatic","evidence":"write just the number, nothing else, to ./out/kingfisher.txt"}],
        "obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"hey can u","role":"context"},
                   {"text":"count how many kingfisher sightings there r in ./birds/sightings.csv (species column, its written 'Kingfisher')","role":"operation"},
                   {"text":"and write just the number, nothing else, to ./out/kingfisher.txt thx","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}});
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(KINGFISHER).with_authoring_policy(policy());
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
        "the number is written as it is: {out:#?}"
    );
    assert!(
        !keys(&out).contains(&"model"),
        "no language step, no model asked: {out:#?}"
    );
}
