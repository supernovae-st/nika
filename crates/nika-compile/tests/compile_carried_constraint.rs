// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Read on the eco-60 lane of nika-58f0e347 against nika-5d7ed9dc (gpt-5-mini): E04B lost
//! its only "carrier" when the validate over the dedup constraint folded — the constraint
//! « Déduplique les événements entrants par leur identifiant » restates the dedup OBLIGATION
//! the plan carries, which the composer's rule 12 never counted; E20B assembled a `validate`
//! cited as the fragment « validation humaine de ce dossier précis, avant son exécution » of a
//! gate sentence (false READY). A constraint an obligation carries needs no operation; a
//! language step inside a gate sentence is the gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

fn ops(out: &nika_compile::CompileOutcome) -> Vec<String> {
    out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap().to_owned())
        .collect()
}

const DOSSIER: &str = "Quand le bouton Slack de validation est utilisé, retrouve le dossier dans MongoDB, dédoublonne le callback par identifiant et vérifie de nouveau la version courante du dossier avant l’action finale. Marque ensuite le dossier comme approuvé dans MongoDB. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution. Déduplique les événements entrants par leur identifiant ; pas de seconde action pour le même événement.";

fn dossier_proposal() -> Value {
    json!({"steps":[
        {"op":"lookup","detail":"le dossier dans MongoDB","evidence":"retrouve le dossier dans MongoDB"},
        {"op":"validate","detail":"dédoublonner les événements entrants par identifiant : pas de seconde action pour le même événement","evidence":"Déduplique les événements entrants par leur identifiant ; pas de seconde action pour le même événement."}],
        "effects":[{"verb":"update","target":"le dossier comme approuvé dans MongoDB","policy":"human_first","evidence":"Marque ensuite le dossier comme approuvé dans MongoDB"}],
        "obligations":[
        {"kind":"dedup","value":null,"evidence":"dédoublonne le callback par identifiant"},
        {"kind":"revision_check","value":null,"evidence":"vérifie de nouveau la version courante du dossier avant l’action finale"}],
        "constraints":["Déduplique les événements entrants par leur identifiant ; pas de seconde action pour le même événement."],"unknowns":[],
        "regions":[{"text":"Quand le bouton Slack de validation est utilisé,","role":"context"},
                   {"text":"retrouve le dossier dans MongoDB,","role":"operation"},
                   {"text":"dédoublonne le callback par identifiant et vérifie de nouveau la version courante du dossier avant l’action finale.","role":"obligation"},
                   {"text":"Marque ensuite le dossier comme approuvé dans MongoDB.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"},
                   {"text":"Déduplique les événements entrants par leur identifiant ; pas de seconde action pour le même événement.","role":"constraint"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_constraint_the_dedup_obligation_carries_needs_no_operation() {
    let provider = Provider::new(dossier_proposal());
    let req = CompileRequest::create(DOSSIER).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("have no operation to carry them")),
        "{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["lookup"], "{out:#?}");
}

const DOCS: &str = "Recherche les pages de documentation concernées par la version donnée. Répartis leur révision entre plusieurs agents, puis rédige les changements avec leurs références. Chaque agent a au maximum 3 essais. Fusionne ensuite les changements de documentation. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution. Déduplique les événements entrants par leur identifiant ; pas de seconde action pour le même événement.";

fn docs_proposal() -> Value {
    json!({"steps":[
        {"op":"search","detail":"les pages de documentation concernées par la version donnée","evidence":"Recherche les pages de documentation concernées par la version donnée."},
        {"op":"classify","detail":"Répartis leur révision entre plusieurs agents","evidence":"Répartis leur révision entre plusieurs agents","categories":["agent A","agent B"]},
        {"op":"draft","detail":"rédige les changements avec leurs références","evidence":"rédige les changements avec leurs références"},
        {"op":"validate","detail":"validation humaine de ce dossier précis, avant son exécution","evidence":"validation humaine de ce dossier précis, avant son exécution"}],
        "effects":[{"verb":"merge","target":"les changements de documentation","policy":"human_first","evidence":"Fusionne ensuite les changements de documentation."}],
        "obligations":[
        {"kind":"retry_bound","value":3,"evidence":"Chaque agent a au maximum 3 essais"},
        {"kind":"dedup","value":null,"evidence":"Déduplique les événements entrants par leur identifiant"}],
        "constraints":[],"unknowns":[],
        "regions":[{"text":"Recherche les pages de documentation concernées par la version donnée.","role":"operation"},
                   {"text":"Répartis leur révision entre plusieurs agents,","role":"operation"},
                   {"text":"puis rédige les changements avec leurs références.","role":"operation"},
                   {"text":"Chaque agent a au maximum 3 essais.","role":"obligation"},
                   {"text":"Fusionne ensuite les changements de documentation.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"},
                   {"text":"Déduplique les événements entrants par leur identifiant ; pas de seconde action pour le même événement.","role":"obligation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_validate_cited_as_a_fragment_of_the_gate_sentence_is_the_gate() {
    let provider = Provider::new(docs_proposal());
    let req = CompileRequest::create(DOCS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(ops(&out), ["search", "classify", "draft"], "{out:#?}");
}
