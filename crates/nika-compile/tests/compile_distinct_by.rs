// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The removal of duplicates by stated key columns is a typed stage of the computation
//! (sealed sv3-02, every lane: « vire les entrées qui ont le meme titre ET le meme artiste
//! qu'une entrée précédente (garde la 1ere), garde l'ordre » ended in a clarification — the
//! seat's computation could not say the key, and « garde l'ordre » had no carrier). The seat
//! now names `distinct_by`, the compiler validates every key against the request's columns,
//! the jq keeps the first occurrence in place with every column, and the order constraint is
//! carried by the compute task, which keeps the source order by construction.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const RADIO: &str = "yo dans ./radio/diffusions.json (liste d'objets titre/artiste/heure) y a des doublons, vire les entrées qui ont le meme titre ET le meme artiste qu'une entrée précédente (garde la 1ere), garde l'ordre, et écris la liste qui reste dans ./out/uniques.json";

fn radio_proposal(keys: &[&str]) -> Value {
    json!({"steps":[
        {"op":"read","detail":"./radio/diffusions.json (liste d'objets titre/artiste/heure)","evidence":"./radio/diffusions.json (liste d'objets titre/artiste/heure)"},
        {"op":"compute","detail":"vire les entrées qui ont le meme titre ET le meme artiste qu'une entrée précédente (garde la 1ere)","evidence":"vire les entrées qui ont le meme titre ET le meme artiste qu'une entrée précédente (garde la 1ere)",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[],"group_by":"","aggregations":[],"sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[],"distinct_by":keys}}],
        "effects":[{"verb":"write","target":"./out/uniques.json","policy":"automatic","evidence":"écris la liste qui reste dans ./out/uniques.json"}],
        "obligations":[],
        "constraints":["garde l'ordre"],"unknowns":[],
        "regions":[{"text":"yo dans ./radio/diffusions.json (liste d'objets titre/artiste/heure) y a des doublons,","role":"operation"},
                   {"text":"vire les entrées qui ont le meme titre ET le meme artiste qu'une entrée précédente (garde la 1ere),","role":"operation"},
                   {"text":"garde l'ordre,","role":"constraint"},
                   {"text":"et écris la liste qui reste dans ./out/uniques.json","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn duplicates_by_two_stated_keys_are_a_computation_the_compiler_writes() {
    let provider = Provider::new(radio_proposal(&["titre", "artiste"]));
    let req = CompileRequest::create(RADIO).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).is_empty(), "{out:#?}");
    let candidate = out.candidate.as_deref().expect("a candidate");
    assert!(
        candidate.contains("reduce .[] as $r ([]; if any(.[]; .titre == ($r | .titre) and .artiste == ($r | .artiste)) then . else . + [$r] end)"),
        "{candidate}"
    );
    assert!(!candidate.contains("rule_expression"), "{candidate}");
    assert!(!candidate.contains("infer:"), "{candidate}");
    assert!(candidate.contains("./out/uniques.json"), "{candidate}");
    // The order constraint is carried by the compute task, not left unresolved.
    let ledger = out.provenance.decision.as_ref().unwrap()["ledger"].clone();
    let unresolved: Vec<String> = ledger["duties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["state"] == "unresolved")
        .map(|d| d["evidence"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(unresolved.is_empty(), "{unresolved:?}\n{ledger:#}");
    // The recorded plan replays to the same candidate with zero calls.
    let record = out.provenance.plan.clone().unwrap();
    let replayed = compile(&CompileRequest::create(RADIO).with_plan(record)).unwrap();
    assert_eq!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
}

#[tokio::test]
async fn a_key_the_request_never_names_is_no_rule() {
    let provider = Provider::new(radio_proposal(&["titre", "album"]));
    let req = CompileRequest::create(RADIO).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(!candidate.contains("album"), "{candidate}");
}
