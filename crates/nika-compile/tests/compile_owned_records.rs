// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A proposed read of records named by their owner is a lookup (eco-60 E16C on the binary
//! of 2026-09-22, gpt-5-mini: « Lis mes disponibilités et celles des participants » was
//! bound to the material an invocation supplies, and three slots were drafted from an
//! input string, READY). The reader's retrieval cues settle the seat's `read`; the
//! assembler asks where the records live.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const SLOTS: &str = "Lis mes disponibilités et celles des participants, puis propose par écrit trois créneaux compatibles dans le fuseau Europe/Paris. Ne remplace pas une disponibilité absente par un créneau supposé. Arrête-toi après ces étapes ; aucune autre action n'est demandée.";

fn slots_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"mes disponibilités et celles des participants","evidence":"Lis mes disponibilités et celles des participants"},
        {"op":"draft","detail":"trois créneaux compatibles dans le fuseau Europe/Paris","evidence":"propose par écrit trois créneaux compatibles dans le fuseau Europe/Paris"}],
        "effects":[],
        "obligations":[],
        "constraints":["Ne remplace pas une disponibilité absente par un créneau supposé.","Arrête-toi après ces étapes ; aucune autre action n'est demandée."],
        "unknowns":[],
        "regions":[{"text":"Lis mes disponibilités et celles des participants,","role":"operation"},
                   {"text":"puis propose par écrit trois créneaux compatibles dans le fuseau Europe/Paris.","role":"operation"},
                   {"text":"Ne remplace pas une disponibilité absente par un créneau supposé.","role":"constraint"},
                   {"text":"Arrête-toi après ces étapes ; aucune autre action n'est demandée.","role":"constraint"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_read_of_owned_records_is_a_lookup_that_asks_where_they_live() {
    let provider = Provider::new(slots_proposal());
    let req = CompileRequest::create(SLOTS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["lookup", "draft"], "{out:#?}");
    assert!(
        out.candidate.is_none(),
        "no READY before the records' place is known: {out:#?}"
    );
    assert!(
        keys(&out).iter().any(|k| k.ends_with("_directory")),
        "asks where the records live: {out:#?}"
    );
}
