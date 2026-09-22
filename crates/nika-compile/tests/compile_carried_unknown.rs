// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An unknown carried by an element that asks its own question is not unresolved work
//! (sealed lane4 of 2026-09-22, gpt-5-mini: « in die übliche Datei schreiben » and « sorted
//! by the column we agreed on » were filed as unresolved and the request was handed back for
//! a rephrase, where the corpus expects a typed `const.*` question). The write whose target
//! names no file asks its output path; the computation with no rule asks its jq.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const LAGER: &str =
    "./lager/bestand.csv (artikel,menge): Zeilen mit menge unter 5 in die übliche Datei schreiben";

fn lager_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./lager/bestand.csv (artikel,menge)","evidence":"./lager/bestand.csv (artikel,menge)"},
        {"op":"compute","detail":"Zeilen mit menge unter 5","evidence":"Zeilen mit menge unter 5",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"menge","op":"<","value":"5","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","order":"","columns":["artikel","menge"],"derived":[],"limit":"","renames":[]}}],
        "effects":[{"verb":"write","target":"in die übliche Datei schreiben","policy":"automatic","evidence":"in die übliche Datei schreiben"}],
        "obligations":[],"constraints":[],"unknowns":["übliche Datei"],
        "regions":[{"text":"./lager/bestand.csv (artikel,menge):","role":"operation"},
                   {"text":"Zeilen mit menge unter 5","role":"operation"},
                   {"text":"in die übliche Datei schreiben","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn an_alluded_destination_is_asked_as_the_output_path() {
    let provider = Provider::new(lager_proposal());
    let req = CompileRequest::create(LAGER).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        keys(&out).contains(&"const.output_path"),
        "the write asks its own path: {out:#?}"
    );
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(plan["unknowns"], json!([]), "{out:#?}");
}

const VINYL: &str = "Hi there! When you have a moment, could you please read ./vinyl/records.csv (columns id, title, artist, year, plays) and write it to ./out/sorted.csv sorted by the column we agreed on, ascending, same columns? Many thanks!";

fn vinyl_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./vinyl/records.csv (columns id, title, artist, year, plays)","evidence":"read ./vinyl/records.csv (columns id, title, artist, year, plays)"},
        {"op":"compute","detail":"sort ascending by the column we agreed on, keep same columns id, title, artist, year, plays","evidence":"sorted by the column we agreed on, ascending, same columns"}],
        "effects":[{"verb":"write","target":"./out/sorted.csv","policy":"automatic","evidence":"write it to ./out/sorted.csv"}],
        "obligations":[],"constraints":[],"unknowns":["the column we agreed on"],
        "regions":[{"text":"Hi there! When you have a moment, could you please","role":"context"},
                   {"text":"read ./vinyl/records.csv (columns id, title, artist, year, plays)","role":"operation"},
                   {"text":"and write it to ./out/sorted.csv","role":"effect"},
                   {"text":"sorted by the column we agreed on, ascending, same columns?","role":"operation"},
                   {"text":"Many thanks!","role":"context"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn an_alluded_column_is_asked_through_the_computation() {
    let provider = Provider::new(vinyl_proposal());
    let req = CompileRequest::create(VINYL).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        keys(&out).iter().any(|k| k.starts_with("const.")),
        "the computation asks for what it needs: {out:#?}"
    );
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(plan["unknowns"], json!([]), "{out:#?}");
}
