// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A draft that only prepares, formats or serializes the rows a computation produced is no
//! language work: the write takes the computed rows as they are (measured on the sealed-v3
//! treatment lane, 2026-09-22: three such drafts ran under gpt-5-mini and failed at runtime).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

/// sv3-01 as the seat read it: a filter, then a draft « to prepare the filtered CSV
/// content for writing », then the write.
const PRETS: &str = "Veuillez lire le fichier ./bibliotheque/prets.csv (colonnes pret_id, lecteur, titre, jours_retard), conserver uniquement les prêts dont le retard dépasse strictement 14 jours, et écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv.";

fn prets_proposal(draft_detail: &str) -> Value {
    json!({"steps":[
        {"op":"read","detail":"./bibliotheque/prets.csv","evidence":"lire le fichier ./bibliotheque/prets.csv"},
        {"op":"compute","detail":"conserver uniquement les prêts dont le retard dépasse strictement 14 jours","evidence":"conserver uniquement les prêts dont le retard dépasse strictement 14 jours",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[{"field":"jours_retard","op":"gt","value":"14","value_field":""}],"group_by":"",
                        "aggregations":[],"sort_by":"","order":"","columns":["pret_id","lecteur","titre","jours_retard"],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":draft_detail,"evidence":"écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv"}],
        "effects":[{"verb":"write","target":"./out/retards.csv","policy":"automatic","evidence":"écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv"}],
        "obligations":[],"constraints":["avec les mêmes colonnes et dans le même ordre"],"unknowns":[],
        "regions":[{"text":"Veuillez lire le fichier ./bibliotheque/prets.csv (colonnes pret_id, lecteur, titre, jours_retard),","role":"operation"},
                   {"text":"conserver uniquement les prêts dont le retard dépasse strictement 14 jours,","role":"operation"},
                   {"text":"et écrire ces lignes, avec les mêmes colonnes et dans le même ordre, dans ./out/retards.csv.","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_draft_that_only_serializes_computed_rows_is_not_assembled() {
    let provider = Provider::new(prets_proposal(
        "préparer le contenu CSV filtré pour écriture vers ./out/retards.csv en gardant les mêmes colonnes et le même ordre",
    ));
    let req = CompileRequest::create(PRETS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !keys(&out).contains(&"model"),
        "no language step remains, so no model is asked: {out:#?}"
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let source = out.candidate.as_deref().unwrap();
    assert!(!source.contains("infer:"), "{source}");
    assert!(source.contains("jours_retard"), "{source}");
    let ops: Vec<&str> = out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["read", "compute"]);
}

/// The request itself asks for the note: language work, a draft. (A note the seat invents
/// beside « écrire ces lignes … dans ./out/retards.csv » is folded: the request's words decide.)
const PRETS_NOTE: &str = "Veuillez lire le fichier ./bibliotheque/prets.csv (colonnes pret_id, lecteur, titre, jours_retard), conserver uniquement les prêts dont le retard dépasse strictement 14 jours, et rédiger une courte note en français qui résume ces prêts en retard, avec un titre, dans ./out/retards.md.";

#[tokio::test]
async fn a_draft_that_names_language_work_stays_a_draft() {
    // The same shape with a real draft: a French note summarizing the late loans.
    let mut proposal = prets_proposal(
        "rédiger une courte note en français qui résume les prêts en retard, avec un titre",
    );
    let clause = "rédiger une courte note en français qui résume ces prêts en retard, avec un titre, dans ./out/retards.md";
    proposal["steps"][2]["evidence"] = json!(clause);
    proposal["effects"][0]["evidence"] = json!(clause);
    proposal["effects"][0]["target"] = json!("./out/retards.md");
    proposal["constraints"] = json!([]);
    proposal["regions"][2]["text"] = json!(format!("et {clause}."));
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(PRETS_NOTE).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(keys(&out).contains(&"model"), "{out:#?}");
    let ops: Vec<String> = out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ops, ["read", "compute", "draft"]);
}
