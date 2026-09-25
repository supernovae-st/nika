// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A language step over the trigger clause and a step inside a safeguard's words are folded
//! whatever their details name (eco-60 on nika-e02d0aac, gpt-5-mini, another seat sample:
//! E04A came out READY with an `extract` of « the callback and record ids from the button's
//! payload » over « bouton Slack de validation », and a lookup over the revision-check words
//! whose detail named the record kept a second element the request never asked).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const SLACK: &str = "Quand le bouton Slack de validation est utilisé, retrouve le dossier dans MongoDB, dédoublonne le callback par identifiant et vérifie de nouveau la version courante du dossier avant l'action finale. Marque ensuite le dossier comme approuvé dans MongoDB.";

fn slack_proposal() -> Value {
    json!({"steps":[
        {"op":"extract","detail":"Extraire l'identifiant du callback Slack et l'identifiant du dossier depuis le payload du bouton","evidence":"bouton Slack de validation"},
        {"op":"lookup","detail":"Retrouve le dossier dans MongoDB.","evidence":"retrouve le dossier dans MongoDB"},
        {"op":"lookup","detail":"la version courante du dossier dans MongoDB","evidence":"vérifie de nouveau la version courante du dossier"}],
        "effects":[{"verb":"update","target":"Marque ensuite le dossier comme approuvé dans MongoDB","policy":"automatic","evidence":"Marque ensuite le dossier comme approuvé dans MongoDB."}],
        "obligations":[{"kind":"dedup","value":null,"evidence":"dédoublonne le callback par identifiant"},
                       {"kind":"revision_check","value":null,"evidence":"vérifie de nouveau la version courante du dossier avant l'action finale"}],
        "constraints":[],"unknowns":[],
        "regions":[{"text":"Quand le bouton Slack de validation est utilisé,","role":"context"},
                   {"text":"retrouve le dossier dans MongoDB,","role":"operation"},
                   {"text":"dédoublonne le callback par identifiant","role":"obligation"},
                   {"text":"et vérifie de nouveau la version courante du dossier avant l'action finale.","role":"obligation"},
                   {"text":"Marque ensuite le dossier comme approuvé dans MongoDB.","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_language_step_over_the_event_and_a_step_inside_a_safeguard_are_folded() {
    let provider = Provider::new(slack_proposal());
    let req = CompileRequest::create(SLACK).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(
        ops,
        ["lookup"],
        "one lookup, no extract, no second lookup: {out:#?}"
    );
    let kinds: Vec<&str> = plan["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"dedup") && kinds.contains(&"revision_check"),
        "{out:#?}"
    );
    assert!(
        !keys(&out).contains(&"model"),
        "no language step, no model asked: {out:#?}"
    );
}
