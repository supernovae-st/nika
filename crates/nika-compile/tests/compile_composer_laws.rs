// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Three composer and merge laws measured on eco-60 under gpt-5-mini (2026-09-22): a
//! restated clause is not a free constraint, `search` and `lookup` are one retrieval
//! family, and a `write` twin of a `create` over the same words is that create.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider, outcome_document};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy, route};

fn without_infeasibility(out: &nika_compile::CompileOutcome) {
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("is not feasible")),
        "{out:#?}"
    );
    assert!(!keys(out).contains(&"intent.clarification"), "{out:#?}");
    let doc = outcome_document(out);
    assert!(route(&doc).contains("compose: single"), "{}", route(&doc));
}

/// E04A as the seat read it: one lookup, one update, and every other clause restated as a
/// constraint (the trigger, the two safeguards the reader states as obligations, the
/// effect clause itself).
const SLACK: &str = "Quand le bouton Slack de validation est utilisé, retrouve le dossier dans MongoDB, dédoublonne le callback par identifiant et vérifie de nouveau la version courante du dossier avant l'action finale. Marque ensuite le dossier comme approuvé dans MongoDB.";

fn slack_proposal(lookup_evidence: &str) -> Value {
    json!({"steps":[{"op":"lookup","detail":"retrouve le dossier dans MongoDB ; vérifie de nouveau la version courante du dossier","evidence":lookup_evidence}],
           "effects":[{"verb":"update","target":"Marque ensuite le dossier comme approuvé dans MongoDB","policy":"automatic","evidence":"Marque ensuite le dossier comme approuvé dans MongoDB."}],
           "obligations":[{"kind":"dedup","value":null,"evidence":"dédoublonne le callback par identifiant"},{"kind":"revision_check","value":null,"evidence":"vérifie de nouveau la version courante du dossier avant l'action finale"}],
           "constraints":["Quand le bouton Slack de validation est utilisé","retrouve le dossier dans MongoDB","dédoublonne le callback par identifiant","vérifie de nouveau la version courante du dossier avant l'action finale","Marque ensuite le dossier comme approuvé dans MongoDB."],
           "unknowns":[],
           "regions":[{"text":"Quand le bouton Slack de validation est utilisé,","role":"context"},{"text":"retrouve le dossier dans MongoDB,","role":"operation"},{"text":"dédoublonne le callback par identifiant","role":"obligation"},{"text":"et vérifie de nouveau la version courante du dossier avant l'action finale.","role":"obligation"},{"text":"Marque ensuite le dossier comme approuvé dans MongoDB.","role":"effect"}],
           "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_restated_clause_is_not_a_free_constraint() {
    let provider = Provider::new(slack_proposal("retrouve le dossier dans MongoDB"));
    let req = CompileRequest::create(SLACK).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("have no operation to carry them")),
        "{out:#?}"
    );
    without_infeasibility(&out);
}

#[tokio::test]
async fn a_floor_search_is_accounted_for_by_a_candidate_lookup() {
    // The seat anchored its lookup on the trigger clause alone: no overlap with the
    // reader's `search` over « retrouve le dossier dans MongoDB », one retrieval family.
    let provider = Provider::new(slack_proposal(
        "Quand le bouton Slack de validation est utilisé",
    ));
    let req = CompileRequest::create(SLACK).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics.iter().any(|d| d
            .message
            .contains("dropped the recognized operation `search`")),
        "{out:#?}"
    );
    without_infeasibility(&out);
}

/// E03B's final clause as the seat read it: a `create` of the accounting record AND a
/// `write` over the same words, which names no file. « harmonise le ton » keeps the
/// request off the deterministic door, so the seat's proposal is the one judged.
const LEDGER: &str = "Extrais leur numéro et leur total, consulte le registre fournisseur, classe chaque dossier en conforme ou à revoir, puis harmonise le ton des libellés. Enregistre ensuite une écriture dans le logiciel comptable.";

fn ledger_proposal() -> Value {
    json!({"steps":[{"op":"extract","detail":"leur numéro et leur total","evidence":"Extrais leur numéro et leur total"},
                    {"op":"lookup","detail":"le registre fournisseur","evidence":"consulte le registre fournisseur"},
                    {"op":"classify","detail":"chaque dossier en conforme ou à revoir","evidence":"classe chaque dossier en conforme ou à revoir","categories":["conforme","à revoir"]},
                    {"op":"draft","detail":"le ton des libellés","evidence":"harmonise le ton des libellés"}],
           "effects":[{"verb":"create","target":"une écriture dans le logiciel comptable","policy":"automatic","evidence":"Enregistre ensuite une écriture dans le logiciel comptable."},
                      {"verb":"write","target":"Enregistre ensuite une écriture dans le logiciel comptable.","policy":"automatic","evidence":"Enregistre ensuite une écriture dans le logiciel comptable."}],
           "obligations":[],"constraints":[],"unknowns":[],
           "regions":[{"text":"Extrais leur numéro et leur total,","role":"operation"},{"text":"consulte le registre fournisseur,","role":"operation"},{"text":"classe chaque dossier en conforme ou à revoir,","role":"operation"},{"text":"puis harmonise le ton des libellés.","role":"operation"},{"text":"Enregistre ensuite une écriture dans le logiciel comptable.","role":"effect"}],
           "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_write_twin_of_a_create_over_the_same_words_is_that_create() {
    let provider = Provider::new(ledger_proposal());
    let req = CompileRequest::create(LEDGER).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("`write` is not asked by its excerpt")),
        "{out:#?}"
    );
    without_infeasibility(&out);
    let doc = outcome_document(&out);
    let effects = doc["provenance"]["plan"]["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 1, "{doc}");
    assert_eq!(effects[0]["verb"], "create");
}
